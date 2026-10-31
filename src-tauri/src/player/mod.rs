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
pub(crate) mod wire;

use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use prosody_player::{Engine, Mixer};
use prosody_synth::SampleRate;
use serde::Serialize;
use tauri::State;

use crate::media::{Media, Silent};
use host::{AudioSide, Shared};
use wire::{Message, Session};

/// Room in the channel to the audio callback: half a second of 2 048-frame blocks is 12.
const IN_FLIGHT: usize = 64;

/// How long the render thread sleeps between rounds when nothing wakes it: well inside the lead,
/// and the output's report moves on at least this often.
const ROUND: Duration = Duration::from_millis(5);

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

/// An open player: the shared state, the output stream and the render thread.
struct Running {
    shared: Arc<Shared>,
    stream: Option<cpal::Stream>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::Relaxed);
        drop(self.stream.take());
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
        let (device, config) = output::device()?;
        let rate = config.sample_rate;
        let engine = Engine::new(
            SampleRate::new(rate).map_err(|error| error.to_string())?,
            usize::try_from(block).unwrap_or(2_048),
            lead_frames(lead, rate)?,
        );
        let (to_audio, from) = sync_channel(IN_FLIGHT);
        let (back, spent) = sync_channel(IN_FLIGHT);
        let shared = Arc::new(Shared::new(engine, to_audio));
        let audio = AudioSide {
            mixer: Mixer::new(f64::from(rate)),
            from,
            back,
            channels: usize::from(config.channels),
        };
        let stream = output::start(&device, config, audio, Arc::clone(&shared))?;
        let renders = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("prosody-render".to_owned())
            .spawn(move || render(&renders, &spent))
            .map_err(|error| format!("the render thread would not start: {error}"))?;
        *running = Some(Running {
            shared,
            stream: Some(stream),
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
        let session = {
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
            session
        };
        if let Some(session) = session {
            self.media().show(&session);
        }
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
        running
            .as_ref()
            .map_or_else(|| "null".to_owned(), |running| running.shared.poll())
    }

    /// A button of the notification, the lock screen or a headset ([`Shared::button`]).
    #[cfg_attr(not(target_os = "android"), allow(dead_code))] // only Android's media session calls it
    pub(crate) fn button(&self, action: &str) {
        let stopped = {
            let running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(running) = running.as_ref() else {
                return;
            };
            let stopped = running.shared.button(action);
            if let Some(thread) = &running.thread {
                thread.thread().unpark();
            }
            stopped
        };
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

/// The render thread: a round every [`ROUND`], or sooner when a command wakes it.
fn render(shared: &Shared, spent: &Receiver<Vec<f32>>) {
    let mut spare = Vec::new();
    let mut outbox = VecDeque::new();
    let mut said = (0, 0);
    while !shared.quit.load(Ordering::Relaxed) {
        spare.extend(spent.try_iter());
        shared.pump(&mut spare, &mut outbox);
        // Said in the system's log too, with when: the page may be asleep, and a phone's log is
        // where a dropout with the screen off can be found afterwards.
        let now = shared.status.losses();
        if now != said {
            log::warn!(
                "the native player ran dry {} time(s), the output glitched {} time(s), {} frames queued",
                now.0,
                now.1,
                shared.status.queued()
            );
            said = now;
        }
        std::thread::park_timeout(ROUND);
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
    use super::lead_frames;

    #[test]
    fn a_lead_is_whole_frames_and_nothing_else() {
        assert_eq!(lead_frames(0.5, 48_000), Ok(24_000));
        assert_eq!(lead_frames(0.5, 44_100), Ok(22_050));
        assert!(lead_frames(-1.0, 48_000).is_err());
        assert!(lead_frames(f64::NAN, 48_000).is_err());
    }
}
