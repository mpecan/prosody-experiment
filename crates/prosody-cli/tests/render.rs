//! `prosody render`: the piece through the player's synth to a WAV file (`docs/player.md` §7, step
//! 2), and golden hashes of the corpus rendered with oscillators (`docs/player.md` §6).
//!
//! A changed hash is a changed sound. Before accepting one, render the poem and listen:
//! `prosody render --out poem.wav poem.poem`, with the header below.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// Every pitched layer on an oscillator — the sawtooth bass and melody of the `subtractive` synth
/// and square chords — so the whole pitched piece is in the render: the CLI decodes no samples.
const OSCILLATORS: &str = "+++\nsynth = \"subtractive\"\nchord_sound = \"square\"\n+++\n";

/// `poem` with every pitched layer on an oscillator: [`OSCILLATORS`] as its header, or, for a
/// poem with a header of its own (the Slovenian one's sets its scale), its keys added at the end of
/// that one, in whatever table it ends in.
fn on_oscillators(poem: &str) -> String {
    let keys = OSCILLATORS.trim_start_matches("+++\n");
    match poem
        .strip_prefix("+++\n")
        .and_then(|rest| rest.split_once("+++\n"))
    {
        Some((header, body)) => format!("+++\n{header}{keys}{body}"),
        None => format!("{OSCILLATORS}{poem}"),
    }
}

/// A scratch directory of the test's own.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("prosody-render-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs `prosody` with `arguments`, and returns its stderr and exit code.
fn prosody(arguments: &[&str]) -> (String, Option<i32>) {
    let finished = Command::new(env!("CARGO_BIN_EXE_prosody"))
        .args(arguments)
        .output()
        .expect("the binary should run");
    (
        String::from_utf8_lossy(&finished.stderr).into_owned(),
        finished.status.code(),
    )
}

/// Renders `poem` and returns the WAV's bytes and what the CLI said.
fn render(poem: &str, name: &str) -> (Vec<u8>, String) {
    let dir = scratch(name);
    let source = dir.join("poem.poem");
    let out = dir.join("out.wav");
    std::fs::write(&source, poem).unwrap();
    let (err, code) = prosody(&[
        "render",
        "--out",
        out.to_str().unwrap(),
        source.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(0), "{err}");
    let bytes = std::fs::read(&out).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    (bytes, err)
}

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../prosody-emit/tests/corpus")
}

#[test]
fn a_render_is_a_wav_file_of_the_piece_and_says_what_it_left_out() {
    let poem = on_oscillators(&std::fs::read_to_string(corpus().join("pentameter.poem")).unwrap());
    let (wav, err) = render(&poem, "pentameter");
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    // 48 kHz, stereo, 16 bits.
    assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 48_000);
    assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 2);
    assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16);
    // Not silent: the oscillators played.
    let loudest = wav[44..]
        .chunks(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]).unsigned_abs())
        .max()
        .unwrap();
    assert!(loudest > 3_000, "{loudest}");
    // The drums are named as left out, not dropped silently.
    assert!(err.contains("notes played"), "{err}");
    assert!(
        err.contains("drum and 0 soundfont notes are not in the render"),
        "{err}"
    );
}

#[test]
fn the_same_poem_renders_the_same_bytes() {
    // I1, extended to audio (docs/player.md §2).
    let poem = on_oscillators("the fire will burn the night\nand turn the dark to light\n");
    assert_eq!(render(&poem, "same-a").0, render(&poem, "same-b").0);
}

#[test]
fn a_render_needs_somewhere_to_go_and_nothing_else_to_do() {
    let (err, code) = prosody(&["render"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("needs --out PATH"), "{err}");
    let (err, code) = prosody(&["render", "--out", "x.wav", "--osc"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("takes none of --osc"), "{err}");
    let (err, code) = prosody(&["render", "--out"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("--out needs a path"), "{err}");
    // `--out` belongs to `render`; on its own it is an unknown option.
    let (err, code) = prosody(&["--out", "x.wav"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("unknown option `--out`"), "{err}");
}

#[test]
fn golden_corpus_renders() {
    // Every corpus poem through the oscillators: its WAV hashed. The slovenian and nonsense poems
    // are in for the guessed pronunciations (§9), which the synth plays like any other notes.
    let mut names: Vec<String> = std::fs::read_dir(corpus())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| Path::new(name).extension().is_some_and(|ext| ext == "poem"))
        .collect();
    names.sort();
    let mut lines = Vec::new();
    for name in names {
        let poem = std::fs::read_to_string(corpus().join(&name)).unwrap();
        let (wav, err) = render(&on_oscillators(&poem), &name);
        assert!(
            !err.contains("Diagnostic") && !err.contains("at bytes"),
            "{name}: {err}"
        );
        let played = err
            .lines()
            .find(|line| line.contains("notes played"))
            .and_then(|line| line.split(", ").nth(1))
            .unwrap_or("?")
            .to_owned();
        lines.push(format!(
            "{name}: {} bytes, {played}, blake3 {}",
            wav.len(),
            blake3::hash(&wav).to_hex()
        ));
    }
    insta::assert_snapshot!(lines.join("\n"));
}
