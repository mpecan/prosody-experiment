//! The native player (`docs/player.md` §13, design step 5): the same engine the browser's render
//! worker runs (`prosody-player`), driven by a thread of the app and played through the phone's own
//! audio output, so the music and the reading go on with the screen off and the web view paused.
//!
//! The page drives it as it drives the worker — the same messages ([`wire::Message`]) through
//! [`player_post`] — and asks how it is doing with [`player_poll`] rather than being told: a
//! paused web view would otherwise pile up a call for every report while it slept. The worklet's
//! messages (the stems' gains, the browser voice's duck) come through [`player_mix`].

mod host;
mod output;
mod rounds;
pub(crate) mod wire;

use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Instant;

use prosody_player::Engine;
use prosody_synth::SampleRate;
use serde::Serialize;
use tauri::State;

use crate::media::{Media, Silent};
use host::{IN_FLIGHT, Shared};
use output::Output;
use rounds::{ROUND, Rounds, Sink, Wait};
use wire::{Message, Session};

/// The native player: nothing until the page opens it.
pub(crate) struct Native {
    running: Mutex<Option<Running>>,
    media: Mutex<Arc<dyn Media>>,
}

impl Default for Native {
    fn default() -> Self {
        Self {
            running: Mutex::new(None),
            media: Mutex::new(Arc::new(Silent)),
        }
    }
}

/// An open player: the shared state and the render thread, which holds the output.
struct Running {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

impl Native {
    /// What shows the notification and the lock screen's controls.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))] // only Android's media bridge sets one
    pub(crate) fn set_media(&self, media: Arc<dyn Media>) {
        *self.media.lock().unwrap_or_else(PoisonError::into_inner) = media;
    }

    fn media(&self) -> Arc<dyn Media> {
        Arc::clone(&self.media.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Opens the player on the default output, rendering `block` frames at a time `lead` seconds
    /// ahead, and gives the output's rate. A player already open is closed first: a page that opens
    /// one is a new page (a reload), whose sounds are numbered afresh.
    ///
    /// # Errors
    ///
    /// No output to open, or none the synth can run at.
    pub(crate) fn open(&self, block: u32, lead: f64) -> Result<u32, String> {
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        drop(running.take());
        self.media().show(&Session::Stopped);
        let (device, config) = output::device(None)?;
        let rate = config.sample_rate;
        let engine = Engine::new(
            SampleRate::new(rate).map_err(|error| error.to_string())?,
            usize::try_from(block).unwrap_or(2_048),
            lead_frames(lead, rate)?,
        );
        let (to_audio, from) = sync_channel(IN_FLIGHT);
        let (back, spent) = sync_channel(IN_FLIGHT);
        let shared = Arc::new(Shared::new(engine, to_audio));
        let mut output = Output::start(&device, config, from, back, &shared)?;
        let renders = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("prosody-render".to_owned())
            .spawn(move || render(&renders, &spent, &mut output))
            .map_err(|error| format!("the render thread would not start: {error}"))?;
        *running = Some(Running {
            shared,
            thread: Some(thread),
        });
        Ok(rate)
    }

    /// Does what the page's message says.
    ///
    /// # Errors
    ///
    /// The player is not open, or the message could not be read or loaded.
    pub(crate) fn post(&self, message: Message) -> Result<(), String> {
        let running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        let running = running.as_ref().ok_or("the player is not open")?;
        let session = {
            let mut engine = running
                .shared
                .engine
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let session = message.apply(&mut engine);
            for event in engine.drain() {
                running.shared.tell(event.json());
            }
            session?
        };
        if let Some(thread) = &running.thread {
            thread.thread().unpark();
        }
        // Told while the player is still held, so the notification hears of starts and stops in
        // the order they happened: a button's stop cannot reach it after a later Play.
        if let Some(session) = session {
            self.media().show(&session);
        }
        Ok(())
    }

    /// The page was shown or hidden ([`Shared::view`]).
    ///
    /// # Errors
    ///
    /// The player is not open.
    pub(crate) fn view(&self, visible: bool) -> Result<(), String> {
        let running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        let running = running.as_ref().ok_or("the player is not open")?;
        running.shared.view(visible);
        Ok(())
    }

    /// The mix the page asks for: the stems' gains, the browser voice's duck.
    ///
    /// # Errors
    ///
    /// The player is not open.
    pub(crate) fn mix(&self, gains: Option<&[f64]>, duck: Option<f64>) -> Result<(), String> {
        let running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        let running = running.as_ref().ok_or("the player is not open")?;
        if let Some(gains) = gains {
            running.shared.mix.set_gains(gains);
        }
        if let Some(duck) = duck {
            running.shared.mix.set_duck(duck);
        }
        Ok(())
    }

    /// The output's report and the events since the page last asked, as JSON; `null` when the
    /// player is not open.
    pub(crate) fn poll(&self) -> String {
        let running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        running.as_ref().map_or_else(
            || "null".to_owned(),
            |running| running.shared.poll(Instant::now()),
        )
    }

    /// A button of the notification, the lock screen or a headset ([`Shared::button`]).
    #[cfg_attr(not(target_os = "android"), allow(dead_code))] // only Android's media session calls it
    pub(crate) fn button(&self, action: &str) {
        let running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(running) = running.as_ref() else {
            return;
        };
        let stopped = running.shared.button(action, Instant::now());
        if let Some(thread) = &running.thread {
            thread.thread().unpark();
        }
        // Under the lock, as in `post`.
        if stopped {
            self.media().show(&Session::Stopped);
        }
    }
}

/// `lead` seconds as frames at `rate`.
fn lead_frames(lead: f64, rate: u32) -> Result<u64, String> {
    let frames = (lead * f64::from(rate)).round();
    (0.0..1e12)
        .contains(&frames)
        .then(|| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // checked above
            let frames = frames as u64;
            frames
        })
        .ok_or_else(|| format!("a lead of {lead} s is not a lead"))
}

/// The render thread: a round every [`ROUND`] while something plays, or sooner when a command
/// wakes it; asleep until one does while nothing plays. It holds the output, and drops it as it ends.
fn render(shared: &Arc<Shared>, spent: &Receiver<Vec<f32>>, output: &mut impl Sink) {
    let mut rounds = Rounds::default();
    while !shared.quit.load(Ordering::Relaxed) {
        match rounds.round(shared, spent, output) {
            Wait::Round => std::thread::park_timeout(ROUND),
            Wait::Asleep => std::thread::park(),
        }
    }
}

/// The output's rate, once open.
#[derive(Debug, Serialize)]
pub(crate) struct Opened {
    rate: u32,
}

/// Opens the native player (see [`Native::open`]).
///
/// # Errors
///
/// No output to open.
#[tauri::command]
pub(crate) async fn player_open(
    native: State<'_, Native>,
    block: u32,
    lead: f64,
) -> Result<Opened, String> {
    native.open(block, lead).map(|rate| Opened { rate })
}

/// A message for the native player (see [`Native::post`]).
///
/// # Errors
///
/// The player is not open, or the message could not be read.
#[tauri::command]
pub(crate) async fn player_post(native: State<'_, Native>, message: Message) -> Result<(), String> {
    native.post(message)
}

/// The mix (see [`Native::mix`]).
///
/// # Errors
///
/// The player is not open.
#[tauri::command]
pub(crate) async fn player_mix(
    native: State<'_, Native>,
    gains: Option<Vec<f64>>,
    duck: Option<f64>,
) -> Result<(), String> {
    native.mix(gains.as_deref(), duck)
}

/// The page was shown or hidden (see [`Native::view`]).
///
/// # Errors
///
/// The player is not open.
#[tauri::command]
pub(crate) async fn player_view(native: State<'_, Native>, visible: bool) -> Result<(), String> {
    native.view(visible)
}

/// How the player is doing (see [`Native::poll`]).
///
/// # Errors
///
/// None; a `Result` because Tauri requires one of a command that borrows state.
#[tauri::command]
pub(crate) async fn player_poll(native: State<'_, Native>) -> Result<String, String> {
    Ok(native.poll())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, Weak};

    use super::host::tests::rig;
    use super::wire::{Message, Session};
    use super::{Native, Running, lead_frames};
    use crate::media::Media;

    /// Records each session shown, and whether the player was held while it was.
    struct Witness {
        native: Mutex<Weak<Native>>,
        seen: Mutex<Vec<(Session, bool)>>,
    }

    impl Media for Witness {
        fn show(&self, session: &Session) {
            let held = self
                .native
                .lock()
                .unwrap()
                .upgrade()
                .is_some_and(|native| native.running.try_lock().is_err());
            self.seen.lock().unwrap().push((session.clone(), held));
        }
    }

    #[test]
    fn the_notification_is_told_of_a_start_or_a_stop_while_the_player_is_held() {
        // Held, a button's stop and the page's Play reach the notification in the order they
        // changed the player: the stop cannot be told after a Play that came after it.
        let native = Arc::new(Native::default());
        let witness = Arc::new(Witness {
            native: Mutex::new(Arc::downgrade(&native)),
            seen: Mutex::new(Vec::new()),
        });
        native.set_media(Arc::clone(&witness) as Arc<dyn Media>);
        let (shared, _audio, _spent) = rig();
        *native.running.lock().unwrap() = Some(Running {
            shared: Arc::new(shared),
            thread: None,
        });
        let play: Message = serde_json::from_str(
            r#"{"op":"play","generation":1,"serial":1,"notes":"","bars":"","seconds":2,"cues":"","cast":[],"pace":"natural","duck":0.6,"title":"The Tyger"}"#,
        )
        .unwrap();
        native.post(play).unwrap();
        native.button("pause");
        let stop: Message = serde_json::from_str(r#"{"op":"stop","generation":2}"#).unwrap();
        native.post(stop).unwrap();
        assert_eq!(
            *witness.seen.lock().unwrap(),
            vec![
                (Session::Playing("The Tyger".to_owned()), true),
                (Session::Stopped, true),
                (Session::Stopped, true),
            ]
        );
    }

    #[test]
    fn a_lead_is_whole_frames_and_nothing_else() {
        assert_eq!(lead_frames(0.5, 48_000), Ok(24_000));
        assert_eq!(lead_frames(0.5, 44_100), Ok(22_050));
        assert!(lead_frames(-1.0, 48_000).is_err());
        assert!(lead_frames(f64::NAN, 48_000).is_err());
    }
}
