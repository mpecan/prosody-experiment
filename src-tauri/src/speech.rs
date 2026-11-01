//! The phone's own speech engine as a voice of the reading (`system`, `web/src/reading/engines.ts`):
//! on Android, `TextToSpeech` through `SpeechPlugin` (Kotlin, in the Android project), which renders
//! a line to a WAV file; this reads the file back and hands the page its bytes, so the page gets a
//! line as it gets a neural voice's — audio to place on the beat — with nothing downloaded and no
//! model held in the app. The web view has no speech synthesis of its own.
//!
//! Elsewhere (a desktop, and iOS until its side is written) every command answers that there is no
//! system voice, and the page offers none.

use tauri::ipc::Response;
use tauri::{AppHandle, Runtime};

/// Says so where the phone's speech engine is not bridged.
#[cfg(not(target_os = "android"))]
const NONE: &str = "no system voice here";

/// The bridge, as a Tauri plugin: on Android it loads `SpeechPlugin` for the commands below.
pub(crate) fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    let builder = tauri::plugin::Builder::new("speech");
    #[cfg(target_os = "android")]
    let builder = builder.setup(|app, api| {
        use tauri::Manager;
        let handle = api.register_android_plugin("rocks.prosody", "SpeechPlugin")?;
        app.manage(phone::Speech(handle));
        Ok(())
    });
    builder.build()
}

/// The voices on the phone and the one it reads with, as JSON: `{ voices: [{ id, lang, quality }],
/// default, engine }`.
///
/// # Errors
///
/// No system voice here, or the phone's engine did not start.
#[tauri::command]
pub(crate) async fn speech_voices<R: Runtime>(
    app: AppHandle<R>,
) -> Result<serde_json::Value, String> {
    #[cfg(target_os = "android")]
    return phone::call(app, "voices", serde_json::json!({})).await;
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err(NONE.to_owned())
    }
}

/// `text` in `voice` at `speed` (1 the voice's own), as the WAV file's bytes.
///
/// # Errors
///
/// No system voice here, no such voice, or the engine could not read the line.
#[tauri::command]
pub(crate) async fn speech_render<R: Runtime>(
    app: AppHandle<R>,
    text: String,
    voice: String,
    speed: f32,
) -> Result<Response, String> {
    #[cfg(target_os = "android")]
    {
        let answer = phone::call(
            app,
            "render",
            serde_json::json!({ "text": text, "voice": voice, "speed": speed }),
        )
        .await?;
        let path = answer
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or("the phone's speech engine gave no file")?
            .to_owned();
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("the line's file could not be read: {error}"));
        // Read or not, the file has done its work: the page keeps the line, the cache does not.
        let _ = std::fs::remove_file(&path);
        Ok(Response::new(bytes?))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, text, voice, speed);
        Err(NONE.to_owned())
    }
}

/// Lets the phone's engine go while the page is hidden and silent; the next call starts it again.
///
/// # Errors
///
/// No system voice here.
#[tauri::command]
pub(crate) async fn speech_release<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    #[cfg(target_os = "android")]
    return phone::call(app, "release", serde_json::json!({}))
        .await
        .map(|_| ());
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err(NONE.to_owned())
    }
}

#[cfg(target_os = "android")]
mod phone {
    use tauri::plugin::PluginHandle;
    use tauri::{AppHandle, Manager, Runtime};

    /// `SpeechPlugin`, once loaded.
    pub(super) struct Speech<R: Runtime>(pub(super) PluginHandle<R>);

    /// Calls `SpeechPlugin.<command>` on a thread of its own: a call into Kotlin waits for its
    /// answer, and a line takes the engine a fraction of a second to render.
    pub(super) async fn call<R: Runtime>(
        app: AppHandle<R>,
        command: &'static str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        tauri::async_runtime::spawn_blocking(move || {
            let speech = app
                .try_state::<Speech<R>>()
                .ok_or("the phone's speech engine is not bridged")?;
            speech
                .0
                .run_mobile_plugin::<serde_json::Value>(command, args)
                .map_err(|error| error.to_string())
        })
        .await
        .map_err(|error| error.to_string())?
    }
}
