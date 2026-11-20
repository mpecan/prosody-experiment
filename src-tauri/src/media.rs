//! What the phone shows while the native player plays (`docs/player.md` §13, §14): on Android a
//! media notification and lock-screen controls, from a foreground service of type `mediaPlayback` —
//! which is also what Android requires of an app that plays with its screen off — and a media
//! session for the buttons; on iOS the lock screen's Now Playing and its remote commands, and the
//! audio session that lets the app play in the background and stops it for a call. Each is a plugin
//! of the app's own in the platform's language (`MediaPlugin` and `PlaybackService`, Kotlin, in the
//! Android project; `MediaPlugin`, Swift, in `src-tauri/ios`), and both take the same two commands;
//! this is the Rust side of the bridge: it tells them when the music starts and stops, and hears
//! their buttons.
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

/// The bridge, as a Tauri plugin: on a phone it loads the platform's `MediaPlugin` and gives the
/// native player a [`Media`] that drives it; elsewhere it does nothing.
pub(crate) fn plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    let builder = tauri::plugin::Builder::new("media");
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let builder = builder.setup(|app, api| {
        phone::start(app, &api)?;
        Ok(())
    });
    builder.build()
}

// The Swift plugin's entry point (`init_plugin_media` in `ios/Sources/MediaPlugin.swift`).
#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_media);

#[cfg(any(target_os = "android", target_os = "ios"))]
mod phone {
    use std::sync::Arc;
    use std::sync::mpsc::{Sender, channel};

    use serde::Serialize;
    use tauri::ipc::{Channel, InvokeResponseBody};
    use tauri::plugin::{PluginApi, PluginHandle};
    use tauri::{AppHandle, Manager, Runtime};

    use super::Media;
    use crate::player::Native;
    use crate::player::wire::Session;

    /// The music's state, as `MediaPlugin.update` reads it.
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

    /// A button, as the platform's side sends it.
    #[derive(serde::Deserialize)]
    struct Button {
        action: String,
    }

    /// The platform's side, called from a thread of its own: a call into Kotlin or Swift waits for
    /// its answer, and the player's commands must not wait on the phone's main thread.
    struct Phone {
        updates: std::sync::Mutex<Sender<Session>>,
    }

    impl Media for Phone {
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
        let handle = register(api)?;
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
                        log::error!("the lock screen's controls could not be updated: {error}");
                    }
                }
            })?;
        app.state::<Native>().set_media(Arc::new(Phone {
            updates: std::sync::Mutex::new(updates),
        }));
        Ok(())
    }

    #[cfg(target_os = "android")]
    fn register<R: Runtime>(
        api: &PluginApi<R, ()>,
    ) -> Result<PluginHandle<R>, tauri::plugin::mobile::PluginInvokeError> {
        api.register_android_plugin("rocks.prosody", "MediaPlugin")
    }

    #[cfg(target_os = "ios")]
    fn register<R: Runtime>(
        api: &PluginApi<R, ()>,
    ) -> Result<PluginHandle<R>, tauri::plugin::mobile::PluginInvokeError> {
        api.register_ios_plugin(super::init_plugin_media)
    }
}
