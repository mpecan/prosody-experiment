//! Compiles the generated tree-sitter parser.

/// Builds `src/parser.c` into a static library for the binding to link against.
fn main() {
    let src = std::path::Path::new("src");
    let mut build = cc::Build::new();
    // `wasm32-unknown-unknown` has no C library, and the generated parser includes `stdlib.h`.
    // `tree-sitter-language` ships the minimal headers the runtime itself builds against there, and
    // hands their path to its direct dependents — this crate — so the browser build of the page
    // (`prosody-wasm`) compiles the same parser as every other target.
    if std::env::var("TARGET").is_ok_and(|target| target.starts_with("wasm32-unknown"))
        && let Ok(headers) = std::env::var("DEP_TREE_SITTER_LANGUAGE_WASM_HEADERS")
    {
        build.include(headers);
    }
    build
        .include(src)
        .file(src.join("parser.c"))
        // The generated parser is not our code and does not answer to our lint settings.
        .warnings(false)
        .compile("tree-sitter-prosody");
    println!("cargo:rerun-if-changed=src/parser.c");
    println!("cargo:rerun-if-changed=grammar.js");
}
