//! Compiles the generated tree-sitter parser.

/// Builds `src/parser.c` into a static library for the binding to link against.
fn main() {
    let src = std::path::Path::new("src");
    cc::Build::new()
        .include(src)
        .file(src.join("parser.c"))
        // The generated parser is not our code and does not answer to our lint settings.
        .warnings(false)
        .compile("tree-sitter-prosody");
    println!("cargo:rerun-if-changed=src/parser.c");
    println!("cargo:rerun-if-changed=grammar.js");
}
