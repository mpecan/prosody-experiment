//! The phone app: a native window around the static site. The page compiles poems with the same
//! wasm compiler it runs in a browser, in a worker, so the app holds no compiler of its own — a
//! second one would be a second thing to keep byte-identical (I1) for no measurable gain.
//!
//! It does hold a player of its own (`docs/player.md` §13, design step 5): the engine the page's
//! render worker runs in a browser, here on a thread of the app and played through the phone's own
//! audio output, so playback survives the screen turning off and has the lock screen's controls.
//! The page drives it through the `player_*` commands (`web/src/player/native.ts`).
//!
//! On Android and iOS it also lends the reading the phone's own speech engine (the `speech_*` commands, `src/speech.rs`).

// Tests may unwrap, index and fail with a message; production code may not. See CLAUDE.md §3.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

mod media;
mod player;
mod speech;

/// Where a phone enters the app: `start_app`, which iOS's `main.mm` calls, and Android's JNI entry.
#[cfg(mobile)]
#[allow(missing_docs)] // the entry point's macro adds `start_app` (iOS), which it cannot document
mod entry {
    #[tauri::mobile_entry_point]
    fn start() {
        super::run();
    }
}

/// Opens the app's window on the built site. A failure to start is logged and ends the process:
/// there is nothing to show without the window.
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .manage(player::Native::default())
        .plugin(media::plugin())
        .plugin(speech::plugin())
        .invoke_handler(tauri::generate_handler![
            player::player_open,
            player::player_post,
            player::player_mix,
            player::player_poll,
            player::player_view,
            speech::speech_voices,
            speech::speech_render,
            speech::speech_release,
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        log::error!("prosody could not start: {error}");
        std::process::exit(1);
    }
}
