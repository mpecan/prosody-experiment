//! The desktop entry point of the app; phones enter through `app::run` directly.
// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    app::run();
}
