//! The phone app: a native window around the static site. The page compiles poems with the same
//! wasm compiler it runs in a browser, in a worker, so the app holds no compiler of its own — a
//! second one would be a second thing to keep byte-identical (I1) for no measurable gain.
//!
//! It does hold a player of its own (`docs/player.md` §13, design step 5): the engine the page's
//! render worker runs in a browser, here on a thread of the app and played through the phone's own
//! audio output, so playback survives the screen turning off and has the lock screen's controls.
//! The page drives it through the `player_*` commands (`web/src/player/native.ts`).

// Tests may unwrap, index and fail with a message; production code may not. See CLAUDE.md §3.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

mod media;
mod player;

/// Opens the app's window on the built site. A failure to start is logged and ends the process:
/// there is nothing to show without the window.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .manage(player::Native::default())
        .plugin(media::plugin())
        .invoke_handler(tauri::generate_handler![
            player::player_open,
            player::player_post,
            player::player_mix,
            player::player_poll,
            player::player_view,
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        log::error!("prosody could not start: {error}");
        std::process::exit(1);
    }
}
