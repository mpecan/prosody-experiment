//! Rust binding for the `prosody` tree-sitter grammar.
//!
//! The grammar itself is `grammar.js`; the parser in `src/parser.c` is generated from it and
//! committed so that building this crate needs only a C compiler, not the tree-sitter CLI.

use tree_sitter_language::LanguageFn;

unsafe extern "C" {
    fn tree_sitter_prosody() -> *const ();
}

/// The grammar, for `tree_sitter::Parser::set_language`.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_prosody) };

/// The corpus tests live in `test/corpus` and are run by the tree-sitter CLI; these check only
/// that the grammar loads and parses at all from Rust.
#[cfg(test)]
mod tests {
    #[test]
    fn grammar_loads() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("grammar should load");
        let tree = parser
            .parse("the fire that burns\n", None)
            .expect("should parse");
        assert_eq!(tree.root_node().kind(), "source_file");
        assert!(!tree.root_node().has_error());
    }
}
