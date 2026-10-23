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
    // A binary that refuses its arguments exits without reading the poem, and whether it has
    // exited before this write is a race: on a fast machine the write meets a closed pipe. That is
    // not a failure of the binary, so a broken pipe is let through and the exit code and stderr
    // say what happened.
    let written = child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(poem.as_bytes());
    if let Err(error) = written {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe,
            "writing the poem: {error}"
        );
    }
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
fn the_explain_views_palette_values_share_one_column() {
    // The longest palette keys are 13 characters (density_level, melody_octave, ride_fallback);
    // after the two-space indent every value starts at byte 16.
    let poem = format!("+++\ndensity_level = 1\nride_fallback = \"crash\"\n+++\n{COUPLET}");
    let (out, err, code) = run(&["--explain"], &poem);
    assert_eq!(code, Some(0), "{err}");
    let style: Vec<&str> = out
        .lines()
        .skip_while(|l| *l != "style")
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .collect();
    assert!(style.len() > 10, "{out}");
    for line in style {
        let (key, value) = line.split_at(16);
        assert!(
            key.ends_with(' ') && !value.starts_with(' ') && !value.is_empty(),
            "{line:?} is off the column"
        );
    }
    assert!(out.contains("  density_level 1.00\n"), "{out}");
    assert!(out.contains("  ride_fallback crash\n"), "{out}");
    assert!(out.contains("  bass_octave   2\n"), "{out}");
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
fn the_osc_schedule_names_the_header_keys_it_does_not_apply() {
    let poem = format!("+++\nmelody_octave = 5\n+++\n{COUPLET}");
    let (out, err, code) = run(&["--osc"], &poem);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(
        out.lines().nth(1),
        Some(
            "# not applied here: melody_octave = 5 (pitch is sent from MIDI 36, velocity unscaled)"
        ),
        "{out}"
    );
    let (plain, _, _) = run(&["--osc"], COUPLET);
    assert!(!plain.contains("not applied"), "{plain}");
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

/// A running watcher whose output is collected as it arrives.
///
/// The watcher tests used to sleep a fixed 400 ms and assume the first compile had finished. It
/// had alone, but not while the whole workspace's suites ran beside it, so they failed in most
/// full runs. Waiting for the output itself, under a generous deadline, tests the same thing
/// without guessing how fast the machine is.
struct Watcher {
    child: std::process::Child,
    out: std::sync::Arc<std::sync::Mutex<String>>,
    complaint: std::sync::Arc<std::sync::Mutex<String>>,
}

impl Watcher {
    fn start(arguments: &[&str], poem: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_prosody"))
            .args(arguments)
            .arg(poem)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the binary should start");
        let out = Self::collect(child.stdout.take().unwrap());
        let complaint = Self::collect(child.stderr.take().unwrap());
        Self {
            child,
            out,
            complaint,
        }
    }

    fn collect(
        mut stream: impl std::io::Read + Send + 'static,
    ) -> std::sync::Arc<std::sync::Mutex<String>> {
        let text = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let filling = std::sync::Arc::clone(&text);
        std::thread::spawn(move || {
            let mut chunk = [0_u8; 4096];
            while let Ok(read) = stream.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                filling
                    .lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&chunk[..read]));
            }
        });
        text
    }

    /// Waits until `done(stdout, stderr)` holds, or gives up after a deadline and lets the
    /// assertions that follow say what was missing.
    fn wait_for(&self, done: impl Fn(&str, &str) -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            if done(&self.out.lock().unwrap(), &self.complaint.lock().unwrap()) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// Stops the watcher and hands back what it wrote to stdout and stderr.
    fn stop(mut self) -> (String, String) {
        self.child.kill().expect("stopping the watcher");
        self.child.wait().expect("collecting the watcher");
        let out = self.out.lock().unwrap().clone();
        let complaint = self.complaint.lock().unwrap().clone();
        (out, complaint)
    }
}

#[test]
fn the_watcher_recompiles_when_the_poem_changes() {
    // `SPEC.md` §7.8's live reload, and what invariant I2 was written for.
    let poem = std::env::temp_dir().join("prosody-watch.poem");
    std::fs::write(&poem, COUPLET).expect("writing the poem");

    let watcher = Watcher::start(&["--watch", "--osc"], &poem);
    watcher.wait_for(|out, complaint| complaint.contains("watching") && out.contains("# "));
    std::fs::write(&poem, format!("{COUPLET}and the wind will rise\n")).expect("editing");
    watcher.wait_for(|out, _| out.matches("# ").count() >= 2);
    let (out, complaint) = watcher.stop();
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

    let watcher = Watcher::start(&["--watch"], &poem);
    watcher.wait_for(|out, _| out.contains("stack("));
    std::fs::write(&poem, "+++\nscale = \"klingon\"\n+++\nthe fire burns\n").expect("breaking it");
    watcher.wait_for(|_, complaint| complaint.contains("unknown scale"));
    std::fs::write(&poem, COUPLET).expect("fixing it");
    watcher.wait_for(|out, _| out.matches("stack(").count() >= 2);
    let (out, complaint) = watcher.stop();
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
fn the_machine_profile_adds_pulse_and_steadiness() {
    let (out, err, code) = run(&["--score", "--profile", "machine"], COUPLET);
    assert_eq!(code, Some(0), "{err}");
    let keys: Vec<&str> = out
        .lines()
        .skip(1)
        .take(7)
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
            "pulse",
            "steadiness",
        ]
    );
    // The machine bands, not the interest ones: layering is judged from three voices up.
    assert!(out.contains("3.000..5.000"), "{out}");

    let (_, err, code) = run(&["--score", "--profile", "polka"], COUPLET);
    assert_ne!(code, Some(0));
    assert!(err.contains("unknown profile `polka`"), "{err}");
}

#[test]
fn the_jazz_profile_judges_the_same_piece_against_its_own_bands() {
    let (jazz, err, code) = run(&["--score", "--profile", "jazz"], COUPLET);
    assert_eq!(code, Some(0), "{err}");
    let (machine, _, _) = run(&["--score", "--profile", "machine"], COUPLET);
    // The jazz bands, from `prosody-score`: recurrence 0.65..0.85, syncopation 0.45..0.9.
    assert!(jazz.contains("0.650..0.850"), "{jazz}");
    assert!(jazz.contains("0.450..0.900"), "{jazz}");
    // Same piece, so the same values: only the bands and the fits differ.
    let value = |out: &str, name: &str| {
        out.lines()
            .find(|line| line.starts_with(name))
            .and_then(|line| line.split_whitespace().nth(1))
            .map(str::to_owned)
    };
    for name in [
        "layering",
        "recurrence",
        "syncopation",
        "pulse",
        "steadiness",
    ] {
        assert_eq!(value(&jazz, name), value(&machine, name), "{name}");
    }
}

#[test]
fn a_mapping_can_be_left_out() {
    // `COUPLET`'s two stanzas are one regular foundation, so with every mapping its stressed
    // vowels carry a bass note, and the outro plays the last stanza again.
    let (full, err, code) = run(&[], COUPLET);
    assert_eq!(code, Some(0), "{err}");
    let (bare, err, code) = run(&["--without", "bass,outro"], COUPLET);
    assert_eq!(code, Some(0), "{err}");
    assert_ne!(full, bare);
    // Two stanzas in one layer: two bars stated, two passes of two transforming, two of outro —
    // eight bars, and six without the outro.
    assert!(full.contains("// 8 bars from 4 lines"), "{full}");
    assert!(bare.contains("// 6 bars from 4 lines"), "{bare}");

    let (_, err, code) = run(&["--without", "bass,polka"], COUPLET);
    assert_ne!(code, Some(0));
    assert!(err.contains("unknown mapping `polka`"), "{err}");
}

#[test]
fn set_chooses_variants_and_without_is_its_shorthand() {
    // `--set bass=off,outro=off` is the same piece as `--without bass,outro`: six bars.
    let (bare, err, code) = run(&["--without", "bass,outro"], COUPLET);
    assert_eq!(code, Some(0), "{err}");
    let (set, err, code) = run(&["--set", "bass=off,outro=off"], COUPLET);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(bare, set);
    // Setting a default changes nothing; the reading is not in the pattern, so it changes
    // nothing here either.
    let (full, _, _) = run(&[], COUPLET);
    let (same, err, code) = run(&["--set", "outro=on,reading=stanza"], COUPLET);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(full, same);
    // The explain view lists each choice that is not a default, and only those.
    let (explained, err, code) = run(
        &["--explain", "--set", "reading=stanza,bass=height"],
        COUPLET,
    );
    assert_eq!(code, Some(0), "{err}");
    // `bass=height` is the default, so the block is the one line; then the summary's blank line.
    assert!(
        explained.contains("mappings\n  reading       stanza\n\n"),
        "{explained}"
    );
    let (plain, _, _) = run(&["--explain"], COUPLET);
    assert!(!plain.contains("mappings\n"), "{plain}");

    for (given, expected) in [
        ("polka=on", "unknown mapping `polka`"),
        (
            "bass=stanza",
            "unknown variant `stanza` of mapping `bass` (it has off, height, syllables, feet)",
        ),
        (
            "reading=on",
            "unknown variant `on` of mapping `reading` (it has off, arrives, stanza)",
        ),
        ("reading", "--set wants name=variant, not `reading`"),
    ] {
        let (_, err, code) = run(&["--set", given], COUPLET);
        assert_ne!(code, Some(0), "{given}");
        assert!(err.contains(expected), "{given}: {err}");
    }
    let (_, err, code) = run(&["--set"], COUPLET);
    assert_ne!(code, Some(0));
    assert!(err.contains("--set needs a list"), "{err}");
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

#[test]
fn the_harmony_adds_a_chord_lane_that_is_valid_javascript() {
    // The couplet's stanzas are night/light and sea/me: A on degree 0, B on degree 1, each pair
    // in one stanza. rhyme: each line's class's degree. cadence: the first line of each pair
    // opens (V of its degree), and the second — the stanza's last bar — turns round onto the
    // first chord (V of that root). both: the same, since every line opens or turns round.
    for (variant, expected) in [
        ("rhyme", "1:I/0 2:I/0 3:I/1 4:I/1"),
        ("cadence", "1:V/4 2:V/8 3:V/5 4:V/9"),
        ("both", "1:V/4 2:V/8 3:V/5 4:V/9"),
    ] {
        let set = format!("harmony={variant}");
        let (out, err, code) = run(&["--set", &set], COUPLET);
        assert_eq!(code, Some(0), "{err}");
        assert!(
            out.contains(&format!("//   chords by line: {expected}\n")),
            "{variant}:\n{out}"
        );
        assert!(out.contains("// the chords"), "{out}");
        if let Some(node) = node() {
            let file = std::env::temp_dir().join(format!("prosody-harmony-{variant}.js"));
            std::fs::write(&file, &out).expect("writing the pattern");
            let checked = Command::new(&node)
                .arg("--check")
                .arg(&file)
                .output()
                .expect("node should run");
            let _ = std::fs::remove_file(&file);
            assert!(
                checked.status.success(),
                "node rejected {variant}:\n{}",
                String::from_utf8_lossy(&checked.stderr)
            );
        }
        let (explained, err, code) = run(&["--explain", "--set", &set], COUPLET);
        assert_eq!(code, Some(0), "{err}");
        assert!(
            explained.contains(&format!("  harmony       {variant}\n")),
            "{explained}"
        );
        assert!(explained.contains("  harmony V deg 4 (V of deg 0)") || variant == "rhyme");
    }
    // Off — the default — nothing about chords at all.
    let (out, _, _) = run(&[], COUPLET);
    assert!(!out.contains("chord"), "{out}");
    // The OSC schedule cannot send a chord, and says how many it leaves out.
    let (osc, err, code) = run(&["--osc", "--set", "harmony=rhyme"], COUPLET);
    assert_eq!(code, Some(0), "{err}");
    assert!(osc.contains("# not scheduled here: "), "{osc}");
}
