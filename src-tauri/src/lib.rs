//! The phone app: a native window around the static site. The page compiles poems with the same
//! wasm compiler it runs in a browser, in a worker, so the app holds no compiler of its own — a
//! second one would be a second thing to keep byte-identical (I1) for no measurable gain.

/// Opens the app's window on the built site. A failure to start is logged and ends the process:
/// there is nothing to show without the window.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .run(tauri::generate_context!());
    if let Err(error) = result {
        log::error!("prosody could not start: {error}");
        std::process::exit(1);
    }
}
