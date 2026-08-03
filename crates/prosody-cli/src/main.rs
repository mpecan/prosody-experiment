//! `prosody` — a poem in, a pattern out. `SPEC.md` §10.
//!
//! > CLI first, single responsibility — parse in, pattern out — with the live-reload watcher as a
//! > separate concern.
//!
//! All of this crate's reason to exist is IO. `CLAUDE.md` §2 puts every filesystem read, every
//! write to stdout and every argument here, so that `prosody-core` can be pure by construction
//! rather than by discipline. Nothing in this file decides anything about music.

use std::fmt::Write as _;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context as _, Result, bail};
use prosody_cmudict::CmuDict;
use prosody_core::lower::{self, Analyses};
use prosody_core::voice::Score;
use prosody_core::{Fallback, Poem, meter, repetition, rhyme, role, sound, surprisal};
use prosody_emit::{Backend as _, Strudel, Style, fmt_param};
use prosody_lexicon::EnglishLexicon;
use prosody_lts::LetterToSound;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // `{:#}` prints the whole context chain, which is what makes the contexts added at
            // each IO edge worth adding.
            eprintln!("prosody: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// What the user asked for.
struct Arguments {
    /// The poem to compile, or `None` to read standard input.
    source: Option<PathBuf>,
    /// Print the analysis instead of the pattern.
    explain: bool,
}

const USAGE: &str = "usage: prosody [--explain] [FILE]\n\
     \n\
     Compiles a poem into Strudel pattern text on stdout.\n\
     Reads standard input when FILE is absent.\n\
     \n\
     options:\n\
     \x20 --explain   print what each line became, instead of the pattern\n\
     \x20 -           read the poem from standard input\n\
     \x20 -h, --help  print this message\n";

fn parse_arguments() -> Result<Option<Arguments>> {
    let mut source = None;
    let mut explain = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "-h" | "--help" => return Ok(None),
            "--explain" => explain = true,
            flag if flag.starts_with('-') && flag != "-" => {
                bail!("unknown option `{flag}`\n\n{USAGE}")
            }
            // The conventional name for standard input. Without this, `-` fell through to the
            // path arm and was opened as a file literally called `-`.
            "-" => source = None,
            path => {
                if source.is_some() {
                    bail!("more than one file given\n\n{USAGE}");
                }
                source = Some(PathBuf::from(path));
            }
        }
    }
    Ok(Some(Arguments { source, explain }))
}

fn run() -> Result<()> {
    let Some(arguments) = parse_arguments()? else {
        print!("{USAGE}");
        return Ok(());
    };

    let text = read(arguments.source.as_deref())?;
    let poem = Poem::parse(text).context("the poem could not be parsed")?;

    // Diagnostics go to stderr, so a pattern piped to a file stays a pattern. A poem with problems
    // still compiles — `CLAUDE.md` §6 says a malformed header or an unknown word is a documented
    // fallback, never a refusal.
    for diagnostic in poem.diagnostics() {
        eprintln!(
            "prosody: {:?} at bytes {}..{}",
            diagnostic.kind,
            diagnostic.span.start(),
            diagnostic.span.end()
        );
    }

    let frontmatter = poem.frontmatter().and_then(|span| span.text(poem.source()));
    let style = Style::parse_optional(frontmatter).context("the style header could not be read")?;

    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new())
        .context("the phonemisers disagree about their inventory")?;
    let lexicon = EnglishLexicon::new();

    let meter = meter::scan(&poem, &phonemiser);
    let rhyme = rhyme::analyse(&poem, &phonemiser);
    let repetition = repetition::analyse(&poem);
    let sound = sound::analyse(&poem, &phonemiser);
    let surprisal = surprisal::analyse(&poem, &phonemiser, &lexicon);
    let roles = role::assign(&meter, &repetition).context("role assignment")?;
    let score = lower::run(Analyses {
        poem: &poem,
        meter: &meter,
        rhyme: &rhyme,
        repetition: &repetition,
        sound: &sound,
        surprisal: &surprisal,
        roles: &roles,
    })
    .context("lowering the poem to a score")?;

    if arguments.explain {
        return explain(&poem, &score, &style);
    }

    let pattern = Strudel::new()
        .render(&score, &style)
        .context("rendering the score")?;
    write(&pattern)
}

/// Writes to standard output, treating a closed pipe as success.
///
/// Rust ignores `SIGPIPE`, so `print!` *panics* on `EPIPE` — and `prosody poem.txt | head -1` is
/// the most ordinary thing a user does with a program whose whole job is text on stdout.
/// `CLAUDE.md` §6 forbids panicking on user input, and §9 notes that a panic in a live-reload loop
/// kills the session.
fn write(text: &str) -> Result<()> {
    let mut out = std::io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => other.context("writing to standard output"),
    }
}

/// Reads the poem, from a file or from standard input.
fn read(source: Option<&Path>) -> Result<String> {
    if let Some(path) = source {
        return std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()));
    }
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .context("reading standard input")?;
    Ok(text)
}

/// Prints what each line became — the beginning of `SPEC.md` §8's debug view.
///
/// Not the whole thing: §8 wants every *token* annotated with role, beat position, pitch class,
/// modulation depth and seed, and that is M7. This is the line-and-event grain, which is what M6
/// can honestly show.
fn explain(poem: &Poem, score: &Score, style: &Style) -> Result<()> {
    let mut out = String::new();
    let _ = writeln!(out, "style");
    for (key, value) in style.describe() {
        let _ = writeln!(out, "  {key:<12} {value}");
    }
    let _ = writeln!(
        out,
        "\n{} bars, {} voices, {} events",
        score.bars(),
        score.voices().len(),
        score.events().len()
    );

    let _ = writeln!(out, "\nvoices");
    for voice in score.voices() {
        let text = voice.span().text(poem.source()).unwrap_or("<lost>");
        // The seed is one of the five things SPEC.md §8 asks the debug view to show, and the only
        // place invariant I2 is visible to a reader at all.
        let _ = writeln!(
            out,
            "  {:>3}  {:<10}  {} event(s)  seed {}  {text:?}",
            voice.id().get(),
            format!("{:?}", voice.layer()),
            score.events_of(voice.id()).count(),
            voice.seed(),
        );
    }

    if !score.silent().is_empty() {
        let _ = writeln!(out, "\nno pronunciation found for");
        for span in score.silent() {
            let _ = writeln!(out, "  {:?}", span.text(poem.source()).unwrap_or("<lost>"));
        }
    }

    let _ = writeln!(out, "\nevents");
    for event in score.events() {
        let word = event.span().text(poem.source()).unwrap_or("<lost>");
        let pitch = event.pitch().map_or_else(
            || "-".to_owned(),
            |pitch| {
                let detune = if pitch.detune().is_exact() {
                    String::new()
                } else {
                    format!("+{}", fmt_param(pitch.detune().as_f64()))
                };
                format!("deg {}{detune}", pitch.degree().get())
            },
        );
        // §8 asks for modulation depth by name, and it is §4.3's morphological row — the mapping
        // the spec says earns the project. An earlier version of this view omitted it.
        let family = event.family().map_or_else(
            || "-".to_owned(),
            |family| format!("v{}+{}", family.voice().get(), family.depth().get()),
        );
        let figure = event.anchor().map_or_else(
            || "-".to_owned(),
            |anchor| format!("{anchor:?}").to_lowercase(),
        );
        let _ = writeln!(
            out,
            "  bar {:>2} beat {:>2} {}/{}  gain {}  {:<10}  fam {:<8}  {:<8}  sharp {}{}  {word:?}",
            event.bar().get(),
            event.beat().get(),
            event.subdivision().index(),
            event.subdivision().of(),
            fmt_param(event.velocity().as_f64()),
            pitch,
            family,
            figure,
            fmt_param(event.sharpness().as_f64()),
            if event.rests_on_a_guess() {
                " (guessed)"
            } else {
                ""
            },
        );
    }
    write(&out)
}
