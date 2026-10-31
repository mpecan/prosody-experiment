//! What the phone shows while the native player plays (`docs/player.md` §13): on Android a media
//! notification and lock-screen controls, from a foreground service of type `mediaPlayback` — which
//! is also what Android requires of an app that plays with its screen off — and a media session for
//! the buttons. The service and the session are Kotlin (`PlaybackService`, `MediaPlugin` in the
//! Android project); this is the Rust side of the bridge: it tells them when the music starts and
//! stops, and hears their buttons.
//!
//! On a desktop there is nothing to show ([`Silent`]); the app's window is the control.

use crate::player::wire::Session;

/// Shows what plays.
pub(crate) trait Media: Send + Sync {
    /// The music started, or stopped.
    fn show(&self, session: &Session);
}

/// Shows nothing: a desktop, or a phone before the bridge is up.
pub(crate) struct Silent;

impl Media for Silent {
    fn show(&self, _session: &Session) {}
}

/// The bridge, as a Tauri plugin: on Android it loads the Kotlin `MediaPlugin` and gives the native
/// player a [`Media`] that drives it; elsewhere it does nothing.
pub(crate) fn plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    let builder = tauri::plugin::Builder::new("media");
    #[cfg(target_os = "android")]
    let builder = builder.setup(|app, api| {
        android::start(app, &api)?;
        Ok(())
    });
    builder.build()
}

#[cfg(target_os = "android")]
mod android {
    use std::sync::Arc;
    use std::sync::mpsc::{Sender, channel};

    use serde::Serialize;
    use tauri::ipc::{Channel, InvokeResponseBody};
    use tauri::plugin::{PluginApi, PluginHandle};
    use tauri::{AppHandle, Manager, Runtime};

    use super::Media;
    use crate::player::Native;
    use crate::player::wire::Session;

    /// The notification's state, as `MediaPlugin.update` reads it.
    #[derive(Serialize)]
    struct Update {
        playing: bool,
        title: String,
    }

    /// Where `MediaPlugin.listen` sends the buttons.
    #[derive(Serialize)]
    struct Listen {
        channel: Channel<serde_json::Value>,
    }

    /// A button, as the Kotlin side sends it.
    #[derive(serde::Deserialize)]
    struct Button {
        action: String,
    }

    /// The Kotlin side, called from a thread of its own: a call into Kotlin waits for its answer,
    /// and the player's commands must not wait on the phone's main thread.
    struct Android {
        updates: std::sync::Mutex<Sender<Session>>,
    }

    impl Media for Android {
        fn show(&self, session: &Session) {
            if let Ok(updates) = self.updates.lock() {
                let _ = updates.send(session.clone());
            }
        }
    }

    pub(super) fn start<R: Runtime>(
        app: &AppHandle<R>,
        api: &PluginApi<R, ()>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let handle: PluginHandle<R> =
            api.register_android_plugin("rocks.prosody", "MediaPlugin")?;
        let (updates, pending) = channel::<Session>();
        let buttons = app.clone();
        std::thread::Builder::new()
            .name("prosody-media".to_owned())
            .spawn(move || {
                let channel = Channel::new(move |body| {
                    if let InvokeResponseBody::Json(json) = body
                        && let Ok(button) = serde_json::from_str::<Button>(&json)
                    {
                        buttons.state::<Native>().button(&button.action);
                    }
                    Ok(())
                });
                if let Err(error) = handle.run_mobile_plugin::<()>("listen", Listen { channel }) {
                    log::error!("the media buttons could not be heard: {error}");
                }
                for session in pending {
                    let update = match session {
                        Session::Playing(title) => Update {
                            playing: true,
                            title,
                        },
                        Session::Stopped => Update {
                            playing: false,
                            title: String::new(),
                        },
                    };
                    if let Err(error) = handle.run_mobile_plugin::<()>("update", update) {
                        log::error!("the media notification could not be updated: {error}");
                    }
                }
            })?;
        app.state::<Native>().set_media(Arc::new(Android {
            updates: std::sync::Mutex::new(updates),
        }));
        Ok(())
    }
}
