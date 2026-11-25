//! The phone's own speech engine as a voice of the reading (`system`, `web/src/reading/engines.ts`):
//! `TextToSpeech` on Android and `AVSpeechSynthesizer` on iOS, each through a `SpeechPlugin` in the
//! platform's language (Kotlin, in the Android project; Swift, in `ios/Sources`) that renders a line
//! to a WAV file. This reads the file back and hands the page its bytes, so the page gets a line as
//! it gets a neural voice's — audio to place on the beat — with nothing downloaded and no model held
//! in the app. Android's web view has no speech synthesis of its own; iOS's speaks, but cannot be
//! placed on the beat.
//!
//! On a desktop every command answers that there is no system voice, and the page offers none.

use tauri::ipc::Response;
use tauri::{AppHandle, Runtime};

/// Says so where the phone's speech engine is not bridged.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
const NONE: &str = "no system voice here";

/// The bridge, as a Tauri plugin: on a phone it loads `SpeechPlugin` for the commands below.
pub(crate) fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    let builder = tauri::plugin::Builder::new("speech");
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let builder = builder.setup(|app, api| {
        use tauri::Manager;
        #[cfg(target_os = "android")]
        let handle = api.register_android_plugin("rocks.prosody", "SpeechPlugin")?;
        #[cfg(target_os = "ios")]
        let handle = api.register_ios_plugin(init_plugin_speech)?;
        app.manage(phone::Speech(handle));
        Ok(())
    });
    builder.build()
}

// The Swift plugin's entry point (`init_plugin_speech` in `ios/Sources/SpeechPlugin.swift`).
#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_speech);

/// The voices on the phone and the one it reads with, as JSON: `{ voices: [{ id, lang, name?, tier?,
/// rank, good }], default, engine, improve? }`.
///
/// # Errors
///
/// No system voice here, or the phone's engine did not start.
#[tauri::command]
pub(crate) async fn speech_voices<R: Runtime>(
    app: AppHandle<R>,
) -> Result<serde_json::Value, String> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let answer = phone::call(app, "voices", serde_json::json!({})).await?;
        let voices = answer.get("voices").and_then(serde_json::Value::as_array);
        let good = voices.map_or(0, |all| {
            all.iter()
                .filter(|voice| {
                    voice.get("good").and_then(serde_json::Value::as_bool) == Some(true)
                })
                .count()
        });
        log::info!(
            "speech: {} voices on the phone, {good} of them good; it reads with {}",
            voices.map_or(0, Vec::len),
            answer.get("default").unwrap_or(&serde_json::Value::Null)
        );
        Ok(answer)
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
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
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let began = std::time::Instant::now();
        let answer = phone::call(
            app,
            "render",
            serde_json::json!({ "text": text, "voice": voice, "speed": speed }),
        )
        .await
        // The line's length, not its text: release builds keep these logs, and a poem has no
        // business in the system log or the app's log file.
        .inspect_err(|error| {
            log::warn!(
                "speech: {voice} could not read a {}-char line: {error}",
                text.chars().count()
            );
        })?;
        let path = answer
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or("the phone's speech engine gave no file")?
            .to_owned();
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("the line's file could not be read: {error}"));
        // Read or not, the file has done its work: the page keeps the line, the cache does not.
        let _ = std::fs::remove_file(&path);
        let bytes = bytes?;
        log::info!(
            "speech: {voice} at {speed:.3}, {} chars, {} kB in {:.2} s",
            text.chars().count(),
            bytes.len() / 1024,
            began.elapsed().as_secs_f64()
        );
        Ok(Response::new(bytes))
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
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
    #[cfg(any(target_os = "android", target_os = "ios"))]
    return phone::call(app, "release", serde_json::json!({}))
        .await
        .map(|_| ());
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = app;
        Err(NONE.to_owned())
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
mod phone {
    use tauri::plugin::PluginHandle;
    use tauri::{AppHandle, Manager, Runtime};

    /// `SpeechPlugin`, once loaded.
    pub(super) struct Speech<R: Runtime>(pub(super) PluginHandle<R>);

    /// Calls `SpeechPlugin.<command>` on a thread of its own: a call into Kotlin or Swift waits for its
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
