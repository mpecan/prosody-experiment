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
fn explain_renders_the_poem_with_each_token_annotated() {
    // `SPEC.md` §8: "render the poem with each token annotated by what it became (role, beat
    // position, pitch class, modulation, seed)". Through the real dictionary, so this checks the
    // pipeline is wired up and not only that the renderer works on a stub.
    let (out, err, code) = run(&["--explain"], COUPLET);
    assert_eq!(code, Some(0), "{err}");

    // The poem itself, line by line — the thing §8 asks to be rendered.
    for line in [
        "the fire will burn the night",
        "and turn the dark to light",
        "the river finds the sea",
    ] {
        assert!(out.contains(line), "the poem is missing {line:?}:\n{out}");
    }
    // ...and every word of it annotated.
    for word in ["fire", "burn", "night", "river", "carries"] {
        assert!(out.contains(word), "{word} is unannotated");
    }

    assert!(
        out.contains("foundation") || out.contains("flourish"),
        "role"
    );
    assert!(out.contains("0.0.0/"), "beat position");
    assert!(out.contains("deg "), "pitch class");
    assert!(out.contains("seed "), "seed");
    // Which pass decided each, which is what makes a wrong mapping findable.
    assert!(out.contains("meter,"), "provenance of the values: {out}");
    assert!(out.contains("§4.2"), "the legend cites SPEC.md");
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

#[test]
fn the_osc_backend_emits_a_schedule() {
    // `SPEC.md` §7.8's second backend. The onsets are in beats and the tempo is separate, so a
    // reader can see both.
    let (out, err, code) = run(&["--osc"], COUPLET);
    assert_eq!(code, Some(0), "{err}");
    assert!(out.starts_with("# "), "{out}");
    assert!(out.contains("bpm"), "the tempo is stated: {out}");
    assert!(out.contains("/prosody/note"), "{out}");
    // Beats accumulate across bars of different lengths, which is the thing this backend exists
    // to do — a fixed-length cycle cannot.
    let onsets: Vec<f64> = out
        .lines()
        .skip(1)
        .filter_map(|line| line.split_whitespace().next())
        .filter_map(|token| token.parse().ok())
        .collect();
    assert!(onsets.len() > 4, "{out}");
    assert!(onsets.windows(2).all(|pair| pair[0] <= pair[1]), "in order");
    assert!(
        onsets.last().copied().unwrap_or(0.0) > 3.0,
        "a four-line poem runs past four beats: {onsets:?}"
    );
}

#[test]
fn the_two_backends_agree_about_what_happens() {
    // Different outputs, same piece. The event count in the Strudel header must match the message
    // count in the OSC schedule, or one backend is dropping something the other keeps.
    let (strudel, _, _) = run(&[], COUPLET);
    let (osc, _, _) = run(&["--osc"], COUPLET);
    let events: usize = strudel
        .lines()
        .find_map(|line| {
            line.strip_prefix("// ")?
                .split(", ")
                .nth(2)?
                .strip_suffix(" events")
        })
        .and_then(|count| count.parse().ok())
        .unwrap_or(0);
    let messages: usize = osc
        .lines()
        .next()
        .and_then(|line| line.split(", ").nth(1)?.strip_suffix(" messages"))
        .and_then(|count| count.parse().ok())
        .unwrap_or(0);
    assert!(events > 0, "{strudel}");
    assert_eq!(events, messages, "one message per event");
}

#[test]
fn send_without_an_address_is_refused() {
    let (_, err, code) = run(&["--send"], COUPLET);
    assert_eq!(code, Some(1));
    assert!(err.contains("needs an address"), "{err}");
}

#[test]
fn watch_without_a_file_is_refused() {
    // There is nothing to watch on standard input.
    let (_, err, code) = run(&["--watch"], COUPLET);
    assert_eq!(code, Some(1));
    assert!(err.contains("needs a file"), "{err}");
}

#[test]
fn the_watcher_recompiles_when_the_poem_changes() {
    // `SPEC.md` §7.8's live reload, and what invariant I2 was written for.
    let poem = std::env::temp_dir().join("prosody-watch.poem");
    std::fs::write(&poem, COUPLET).expect("writing the poem");

    let mut child = Command::new(env!("CARGO_BIN_EXE_prosody"))
        .args(["--watch", "--osc"])
        .arg(&poem)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary should start");

    // Give it time for the first compile, then edit the poem and wait for the second.
    std::thread::sleep(std::time::Duration::from_millis(400));
    std::fs::write(&poem, format!("{COUPLET}and the wind will rise\n")).expect("editing");
    std::thread::sleep(std::time::Duration::from_millis(500));
    child.kill().expect("stopping the watcher");

    let finished = child.wait_with_output().expect("collecting output");
    let out = String::from_utf8_lossy(&finished.stdout);
    let complaint = String::from_utf8_lossy(&finished.stderr);
    let _ = std::fs::remove_file(&poem);

    assert!(complaint.contains("watching"), "{complaint}");
    assert!(!complaint.contains("panicked"), "{complaint}");
    let compiles = out.matches("# ").count();
    assert!(
        compiles >= 2,
        "the watcher must recompile after an edit, saw {compiles}:\n{out}"
    );
}

#[test]
fn the_watcher_survives_a_poem_that_will_not_compile() {
    // A half-written poem is the normal case mid-keystroke, not an error worth exiting on.
    let poem = std::env::temp_dir().join("prosody-watch-broken.poem");
    std::fs::write(&poem, COUPLET).expect("writing the poem");

    let mut child = Command::new(env!("CARGO_BIN_EXE_prosody"))
        .args(["--watch"])
        .arg(&poem)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary should start");

    std::thread::sleep(std::time::Duration::from_millis(400));
    std::fs::write(&poem, "+++\nscale = \"klingon\"\n+++\nthe fire burns\n").expect("breaking it");
    std::thread::sleep(std::time::Duration::from_millis(400));
    std::fs::write(&poem, COUPLET).expect("fixing it");
    std::thread::sleep(std::time::Duration::from_millis(400));
    child.kill().expect("stopping the watcher");

    let finished = child.wait_with_output().expect("collecting output");
    let out = String::from_utf8_lossy(&finished.stdout);
    let complaint = String::from_utf8_lossy(&finished.stderr);
    let _ = std::fs::remove_file(&poem);

    assert!(
        complaint.contains("unknown scale"),
        "it says what broke: {complaint}"
    );
    assert!(
        out.matches("stack(").count() >= 2,
        "and keeps going afterwards:\n{out}"
    );
}

/// The examples directory, from this crate.
fn examples() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

#[test]
fn every_example_compiles() {
    // The examples are real poems with real punctuation, so they exercise paths the stripped test
    // fixtures do not — and an example that stops compiling is a regression nobody would notice,
    // because nothing else reads them.
    let mut found = 0;
    for entry in std::fs::read_dir(examples()).expect("the examples directory") {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_none_or(|extension| extension != "poem") {
            continue;
        }
        found += 1;
        for arguments in [vec![], vec!["--explain"], vec!["--osc"]] {
            let finished = Command::new(env!("CARGO_BIN_EXE_prosody"))
                .args(&arguments)
                .arg(&path)
                .output()
                .expect("the binary should run");
            assert!(
                finished.status.success(),
                "{} failed with {arguments:?}:\n{}",
                path.display(),
                String::from_utf8_lossy(&finished.stderr)
            );
            assert!(
                !String::from_utf8_lossy(&finished.stdout).is_empty(),
                "{} produced nothing with {arguments:?}",
                path.display()
            );
        }
    }
    assert!(
        found >= 9,
        "expected the examples to still be there, found {found}"
    );
}

#[test]
fn the_examples_readme_lists_every_example() {
    // A poem added to the directory and not to the table is undocumented; one removed and left in
    // the table is a lie. Both are cheap to prevent.
    let readme = std::fs::read_to_string(examples().join("README.md")).expect("the README");
    for entry in std::fs::read_dir(examples()).expect("the examples directory") {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_none_or(|extension| extension != "poem") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            readme.contains(&name),
            "{name} is not in examples/README.md"
        );
    }
}

#[test]
fn the_examples_exercise_the_cases_they_claim_to() {
    // The README makes specific claims about what these poems demonstrate. If a mapping changes
    // and the claims stop holding, the README becomes fiction — so they are assertions.
    let explain = |name: &str| -> String {
        let finished = Command::new(env!("CARGO_BIN_EXE_prosody"))
            .arg("--explain")
            .arg(examples().join(name))
            .output()
            .expect("the binary should run");
        String::from_utf8_lossy(&finished.stdout).into_owned()
    };

    // Blake's first stanza returns as his last, so those lines share a voice (§4.3's refrain row).
    let blake = explain("blake-tyger.poem");
    assert!(
        blake.matches("refrain of voice").count() >= 4,
        "The Tyger's returning stanza must be heard as a refrain"
    );

    // Whitman is free verse: §4.1 finds nothing regular, so §9's fallback is what anchors it.
    let whitman = explain("whitman-song-of-myself.poem");
    assert!(
        whitman.contains("foundation (mostregular)"),
        "free verse must fall back rather than find a regular stanza"
    );
    assert!(
        !whitman.contains("(regularity)"),
        "and nothing in it should qualify on its own evidence"
    );

    // Carroll and Prešeren are carried by the letter-to-sound fallback, visibly.
    for name in ["carroll-jabberwocky.poem", "preseren-zdravljica.poem"] {
        assert!(
            explain(name).matches("(guessed)").count() > 5,
            "{name} must exercise the letter-to-sound fallback, and say so"
        );
    }
}

#[test]
fn score_reports_every_metric_and_an_edit_per_word() {
    // The scorer's output is read by scripts, so its keys are the contract: every metric, the
    // edit test, and the totals, each as `name value`.
    let (out, err, code) = run(&["--score"], COUPLET);
    assert_eq!(code, Some(0), "{err}");
    let keys: Vec<&str> = out
        .lines()
        .skip(1)
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    assert_eq!(
        keys,
        [
            "layering",
            "recurrence",
            "pitched",
            "pitch_entropy",
            "syncopation",
            "interest",
            "audible",
            "local",
            "explained",
            "legibility",
            "overall",
        ]
    );
    // The couplet has no punctuation, so its words are its whitespace-separated tokens, and the
    // edit test swaps each of them once.
    let words = COUPLET.split_whitespace().count();
    assert!(out.contains(&format!("of {words} edits")), "{out}");
}
