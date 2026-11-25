//! Generates the app's context (its config, icons and capabilities) for `tauri::generate_context!`,
//! and, for iOS, builds and links the app's own Swift plugin (`ios/`, `docs/player.md` §14).
fn main() {
    #[cfg(target_os = "macos")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        ios::link_media_plugin();
    }
    tauri_build::build();
}

/// The Swift side of the media bridge, built as Tauri's own plugins build theirs: the tauri crate's
/// Swift API (`Tauri`) copied beside the package, which names it by that path, and the package built
/// and linked into the app's library by `tauri-utils`, the helper the tauri crate links its own
/// Swift with. Android needs nothing of the kind: its plugin is Kotlin in the Android project, built
/// by Gradle.
#[cfg(target_os = "macos")]
mod ios {
    use std::path::{Path, PathBuf};

    pub(super) fn link_media_plugin() {
        let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
        let Some(api) = std::env::var_os("DEP_TAURI_IOS_LIBRARY_PATH") else {
            println!("cargo::error=the tauri crate did not say where its Swift API is");
            return;
        };
        println!("cargo::rerun-if-env-changed=DEP_TAURI_IOS_LIBRARY_PATH");
        println!("cargo::rerun-if-changed=ios/Package.swift");
        println!("cargo::rerun-if-changed=ios/Sources");
        let copy = manifest.join(".tauri").join("tauri-api");
        if let Err(error) = replace(Path::new(&api), &copy) {
            println!(
                "cargo::error=the tauri crate's Swift API could not be copied to {}: {error}",
                copy.display()
            );
            return;
        }
        tauri_utils::build::link_apple_library("prosody-media", manifest.join("ios"));
    }

    /// `to` made a copy of `from`, without what Swift's own builds of it leave behind.
    fn replace(from: &Path, to: &Path) -> std::io::Result<()> {
        if to.exists() {
            std::fs::remove_dir_all(to)?;
        }
        copy(from, to)
    }

    fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let name = entry.file_name();
            if [".build", "Package.resolved", "Tests"].contains(&name.to_string_lossy().as_ref()) {
                continue;
            }
            if entry.file_type()?.is_dir() {
                copy(&entry.path(), &to.join(&name))?;
            } else {
                std::fs::copy(entry.path(), to.join(&name))?;
            }
        }
        Ok(())
    }
}
