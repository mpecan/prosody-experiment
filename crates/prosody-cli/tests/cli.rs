//! End-to-end tests for the binary, which is the thing a user actually touches.
//!
//! `SPEC.md` §7.6 wants the cheapest possible audition path; §10 wants "parse in, pattern out".
//! Everything here runs the real binary over the real dictionary, because the rest of the suite
//! runs against a spelling-is-pronunciation stub and would not notice the pipeline being wired up
//! wrongly.
//!
//! One of these shells out to `node --check`. The emitted text is JavaScript, and the only honest
//! way to know it parses is to parse it — a bracket count let two syntax errors through, and an
//! earlier version of the emitter produced a program that could not run at all because the
//! separating comma landed inside a `//` comment.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::io::Write as _;
use std::process::{Command, Stdio};

const COUPLET: &str = "the fire will burn the night\n\
                       and turn the dark to light\n\
                       \n\
                       the river finds the sea\n\
                       and carries you and me\n";

/// Runs the binary with the poem on standard input.
fn run(arguments: &[&str], poem: &str) -> (String, String, Option<i32>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_prosody"))
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary should start");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(poem.as_bytes())
        .expect("writing the poem");
    let finished = child.wait_with_output().expect("the binary should finish");
    (
        String::from_utf8_lossy(&finished.stdout).into_owned(),
        String::from_utf8_lossy(&finished.stderr).into_owned(),
        finished.status.code(),
    )
}

#[test]
fn a_poem_on_stdin_becomes_a_pattern() {
    let (out, err, code) = run(&[], COUPLET);
    assert_eq!(code, Some(0), "{err}");
    assert!(out.starts_with("// compiled from a poem"), "{out}");
    assert!(out.contains("stack("), "{out}");
    assert!(out.contains(".cpm("), "{out}");
}

#[test]
fn the_dash_means_standard_input() {
    // The conventional spelling. It was accepted as an option and then opened as a file called
    // `-`, which is the worst of both.
    let (out, err, code) = run(&["-"], COUPLET);
    assert_eq!(code, Some(0), "{err}");
    assert!(out.contains("stack("), "{out}");
}

#[test]
fn the_emitted_pattern_is_valid_javascript() {
    let Some(node) = node() else {
        eprintln!("node not found; skipping the syntax check");
        return;
    };
    for poem in [
        COUPLET,
        "a single line\n",
        "do not go gentle\nrage rage\ndo not go gentle\n",
        "+++\n[style]\nscale = \"phrygian_dominant\"\nroot = \"D\"\n\
         tempo_range = [128, 134]\nswing = 0.12\nbank = \"tr909\"\nsynth = \"karplus\"\n+++\n\
         the fire will burn the night\nand turn the dark to light\n",
    ] {
        let (out, err, code) = run(&[], poem);
        assert_eq!(code, Some(0), "{err}");
        let file = std::env::temp_dir().join(format!("prosody-{}.js", out.len()));
        std::fs::write(&file, &out).expect("writing the pattern");
        let checked = Command::new(&node)
            .arg("--check")
            .arg(&file)
            .output()
            .expect("node should run");
        let _ = std::fs::remove_file(&file);
        assert!(
            checked.status.success(),
            "node rejected the pattern for {poem:?}:\n{}\n{out}",
            String::from_utf8_lossy(&checked.stderr)
        );
    }
}

/// `node`, if this machine has one.
fn node() -> Option<String> {
    Command::new("node")
        .arg("--version")
        .output()
        .ok()
        .filter(|found| found.status.success())
        .map(|_| "node".to_owned())
}

#[test]
fn explain_names_every_mapping_the_spec_asks_for() {
    // `SPEC.md` §8: "render the poem with each token annotated by what it became (role, beat
    // position, pitch class, modulation, seed)".
    let (out, err, code) = run(&["--explain"], COUPLET);
    assert_eq!(code, Some(0), "{err}");
    assert!(
        out.contains("Foundation") || out.contains("Flourish"),
        "role"
    );
    assert!(out.contains("bar  0 beat"), "beat position");
    assert!(out.contains("deg "), "pitch class");
    assert!(out.contains("fam "), "modulation depth");
    assert!(out.contains("seed "), "seed");
    assert!(out.contains("\"burn\""), "the token itself: {out}");
}

#[test]
fn a_broken_pipe_is_not_a_crash() {
    // `prosody poem.txt | head -1` is the most ordinary thing anyone does with this program, and
    // Rust panics on EPIPE unless you handle it.
    let poem = std::env::temp_dir().join("prosody-pipe.poem");
    std::fs::write(&poem, COUPLET).expect("writing the poem");
    let shell = format!(
        "{} {} | head -1",
        env!("CARGO_BIN_EXE_prosody"),
        poem.display()
    );
    let finished = Command::new("sh")
        .arg("-c")
        .arg(&shell)
        .output()
        .expect("sh should run");
    let _ = std::fs::remove_file(&poem);
    let complaint = String::from_utf8_lossy(&finished.stderr);
    assert!(!complaint.contains("panicked"), "{complaint}");
    assert!(finished.status.success(), "{complaint}");
}

#[test]
fn a_poem_with_no_sound_is_refused_with_a_reason() {
    let (_, err, code) = run(&[], "!!! ???\n");
    assert_eq!(code, Some(1));
    assert!(err.contains("nothing to play"), "{err}");
}

#[test]
fn an_unreadable_style_header_names_the_problem() {
    let (_, err, code) = run(&[], "+++\nscale = \"klingon\"\n+++\nthe fire burns\n");
    assert_eq!(code, Some(1));
    assert!(err.contains("unknown scale"), "{err}");
    assert!(
        err.contains("phrygian_dominant"),
        "it lists what it knows: {err}"
    );
}

#[test]
fn a_missing_file_is_an_error_not_a_panic() {
    let (_, err, code) = run(&["/nonexistent/poem.txt"], "");
    assert_eq!(code, Some(1));
    assert!(!err.contains("panicked"), "{err}");
    assert!(err.contains("reading"), "{err}");
}

#[test]
fn an_unknown_option_is_refused_with_the_usage() {
    let (_, err, code) = run(&["--sing"], COUPLET);
    assert_eq!(code, Some(1));
    assert!(err.contains("unknown option"), "{err}");
    assert!(err.contains("usage:"), "{err}");
}

#[test]
fn help_succeeds() {
    let (out, _, code) = run(&["--help"], "");
    assert_eq!(code, Some(0));
    assert!(out.contains("usage:"), "{out}");
}

#[test]
fn a_word_nobody_can_pronounce_is_reported_rather_than_dropped() {
    // CLAUDE.md §6: fallbacks must be visible.
    let (out, err, code) = run(&[], "the fire will burn\nkrapplesnaffle\n");
    assert_eq!(code, Some(0), "{err}");
    assert!(out.contains("stack("), "the poem still plays");
    // The invented word is guessed rather than silent, so it sounds — and says it guessed.
    let (explained, _, _) = run(&["--explain"], "the fire will burn\nkrapplesnaffle\n");
    assert!(explained.contains("(guessed)"), "{explained}");
}

#[test]
fn the_same_poem_always_compiles_to_the_same_bytes() {
    // Invariant I1, through the whole pipeline including the real dictionary.
    let (first, _, _) = run(&[], COUPLET);
    let (second, _, _) = run(&[], COUPLET);
    assert_eq!(first, second);
}
