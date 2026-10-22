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
use prosody_core::annotate;
use prosody_core::lower::{self, Analyses};
use prosody_core::mapping::{Choice, Mapping, Mappings};
use prosody_core::role::PoemRoles;
use prosody_core::voice::Score;
use prosody_core::{
    Fallback, Lexicon, Phonemiser, Poem, meter, repetition, rhyme, role, sound, surprisal,
};
use prosody_emit::osc::Schedule;
use prosody_emit::{Backend as _, Osc, Strudel, Style};
use prosody_lexicon::EnglishLexicon;
use prosody_lts::LetterToSound;
use prosody_score::{DEFAULT_PROBES, Profile, Report, sensitivity};

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

/// Which backend to render with.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    /// Strudel pattern text (`SPEC.md` §7.6).
    Strudel,
    /// A schedule of OSC messages (§7.8).
    Osc,
}

/// What the user asked for.
struct Arguments {
    /// The poem to compile, or `None` to read standard input.
    source: Option<PathBuf>,
    /// Print the analysis instead of the pattern.
    explain: bool,
    /// Print how interesting and how legible the piece measures, instead of the pattern.
    score: bool,
    /// What `--score` judges the piece against.
    profile: Profile,
    /// Which variant of each candidate mapping applies: the defaults, then `--set` and
    /// `--without` in the order given.
    mappings: Mappings,
    /// Which backend.
    target: Target,
    /// Recompile whenever the file changes (§7.8's live reload).
    watch: bool,
    /// Where to send OSC messages, if anywhere.
    send: Option<String>,
}

const USAGE: &str = "usage: prosody [OPTIONS] [FILE]\n\
     \n\
     Compiles a poem into Strudel pattern text on stdout.\n\
     Reads standard input when FILE is absent.\n\
     \n\
     options:\n\
     \x20 --explain      print the poem with each token annotated, instead of the pattern\n\
     \x20 --score        print how the piece measures (interest, legibility), instead of the pattern\n\
     \x20 --profile NAME what --score judges against: interest (the default), machine or jazz\n\
     \x20 --set LIST     choose candidate mappings' variants, comma-separated name=variant:\n\
     \x20                held, refrain, bass, period, outro = off|on (default on);\n\
     \x20                harmony = off|rhyme|cadence|both (default off);\n\
     \x20                reading = off|arrives|stanza (default arrives)\n\
     \x20 --without LIST shorthand for --set name=off, comma-separated names\n\
     \x20 --osc         emit a schedule of OSC messages instead of Strudel text\n\
     \x20 --send ADDR    send the OSC schedule to ADDR (implies --osc), e.g. 127.0.0.1:57120\n\
     \x20 --watch        recompile whenever the file changes\n\
     \x20 -              read the poem from standard input\n\
     \x20 -h, --help     print this message\n";

fn parse_arguments() -> Result<Option<Arguments>> {
    parse(std::env::args().skip(1))
}

/// The argument grammar, separated from the environment so it can be tested.
fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Option<Arguments>> {
    let mut source = None;
    let mut explain = false;
    let mut score = false;
    let mut target = Target::Strudel;
    let mut watch = false;
    let mut send = None;
    let mut profile = Profile::Interest;
    let mut expecting_address = false;
    let mut expecting_profile = false;
    let mut mappings = Mappings::all();
    let mut expecting_mappings = false;
    let mut expecting_choices = false;
    for argument in arguments {
        if expecting_mappings {
            for name in argument.split(',').filter(|name| !name.is_empty()) {
                mappings = mappings.without(known(name)?);
            }
            expecting_mappings = false;
            continue;
        }
        if expecting_choices {
            for item in argument.split(',').filter(|item| !item.is_empty()) {
                mappings = mappings.with(choice(item)?);
            }
            expecting_choices = false;
            continue;
        }
        if expecting_profile {
            profile = Profile::named(&argument)
                .with_context(|| format!("unknown profile `{argument}`\n\n{USAGE}"))?;
            expecting_profile = false;
            continue;
        }
        if expecting_address {
            send = Some(argument);
            target = Target::Osc;
            expecting_address = false;
            continue;
        }
        match argument.as_str() {
            "-h" | "--help" => return Ok(None),
            "--explain" => explain = true,
            "--score" => score = true,
            "--osc" => target = Target::Osc,
            "--watch" => watch = true,
            "--send" => expecting_address = true,
            "--profile" => expecting_profile = true,
            "--without" => expecting_mappings = true,
            "--set" => expecting_choices = true,
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
    if expecting_address {
        bail!("--send needs an address\n\n{USAGE}");
    }
    if expecting_profile {
        bail!("--profile needs a name\n\n{USAGE}");
    }
    if expecting_mappings {
        bail!("--without needs a list of mappings\n\n{USAGE}");
    }
    if expecting_choices {
        bail!("--set needs a list of name=variant\n\n{USAGE}");
    }
    Ok(Some(Arguments {
        source,
        explain,
        score,
        profile,
        mappings,
        target,
        watch,
        send,
    }))
}

/// The mapping called `name`, or an error naming it.
fn known(name: &str) -> Result<Mapping> {
    Mapping::named(name).with_context(|| format!("unknown mapping `{name}`\n\n{USAGE}"))
}

/// One `name=variant` of `--set`, or an error naming what is wrong with it.
fn choice(item: &str) -> Result<Choice> {
    let Some((name, variant)) = item.split_once('=') else {
        bail!("--set wants name=variant, not `{item}`\n\n{USAGE}");
    };
    let mapping = known(name)?;
    mapping.choose_named(variant).with_context(|| {
        let variants: Vec<&str> = mapping.variants().iter().map(|v| v.name()).collect();
        format!(
            "unknown variant `{variant}` of mapping `{name}` (it has {})\n\n{USAGE}",
            variants.join(", ")
        )
    })
}

fn run() -> Result<()> {
    let Some(arguments) = parse_arguments()? else {
        print!("{USAGE}");
        return Ok(());
    };

    if arguments.watch {
        let Some(path) = arguments.source.clone() else {
            bail!("--watch needs a file to watch\n\n{USAGE}");
        };
        return watch(&path, &arguments);
    }
    compile(&read(arguments.source.as_deref())?, &arguments)
}

/// Recompiles the poem whenever it changes — `SPEC.md` §7.8's live reload.
///
/// This is what invariant I2 was written for: "Per-unit seeding + Tree-sitter's dirty-node set
/// means an edit touches only the voices it should — this is what makes it live-codeable rather
/// than render-and-pray." A failed compile prints its reason and keeps watching, because the
/// half-written poem you get mid-keystroke is the normal case, not an error worth exiting on.
///
/// Polls rather than subscribing to filesystem events. `CLAUDE.md` §10 asks what a dependency
/// replaces: `notify` would replace six lines of `metadata`, and bring platform-specific backends,
/// a background thread and a channel to watch exactly one file. At a tenth of a second the poet
/// cannot tell the difference.
fn watch(path: &Path, arguments: &Arguments) -> Result<()> {
    let mut last = None;
    eprintln!("prosody: watching {} — ^C to stop", path.display());
    loop {
        let stamp = std::fs::metadata(path)
            .and_then(|data| data.modified())
            .ok();
        if stamp != last {
            last = stamp;
            match std::fs::read_to_string(path) {
                Ok(text) => {
                    if let Err(error) = compile(&text, arguments) {
                        // Keep watching. The next keystroke usually fixes it.
                        eprintln!("prosody: {error:#}");
                    }
                }
                Err(error) => eprintln!("prosody: reading {}: {error}", path.display()),
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Compiles one poem and writes whatever was asked for.
fn compile(text: &str, arguments: &Arguments) -> Result<()> {
    let poem = Poem::parse(text.to_owned()).context("the poem could not be parsed")?;

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

    let mappings = &arguments.mappings;
    let (score, roles) = lower_poem(&poem, &phonemiser, &lexicon, mappings)?;

    if arguments.score {
        return report(text, &score, arguments, &phonemiser, &lexicon);
    }
    if arguments.explain {
        return explain(&poem, &score, &style, &roles, mappings);
    }

    match arguments.target {
        Target::Strudel => {
            let pattern = Strudel::new()
                .render(&score, &style)
                .context("rendering the score")?;
            emit(&pattern)
        }
        Target::Osc => {
            let schedule = Osc::new()
                .render(&score, &style)
                .context("scheduling the score")?;
            if let Some(address) = &arguments.send {
                send(&schedule, address)?;
            }
            emit(&schedule.to_text())
        }
    }
}

/// Runs every analysis pass and lowers the result — the whole pipeline between a parsed poem and a
/// backend.
fn lower_poem(
    poem: &Poem,
    phonemiser: &impl Phonemiser,
    lexicon: &impl Lexicon,
    mappings: &Mappings,
) -> Result<(Score, PoemRoles)> {
    let meter = meter::scan(poem, phonemiser);
    let rhyme = rhyme::analyse(poem, phonemiser);
    let repetition = repetition::analyse(poem);
    let sound = sound::analyse(poem, phonemiser);
    let surprisal = surprisal::analyse(poem, phonemiser, lexicon);
    let roles = role::assign(&meter, &repetition).context("role assignment")?;
    let score = lower::run_with(
        Analyses {
            poem,
            meter: &meter,
            rhyme: &rhyme,
            repetition: &repetition,
            sound: &sound,
            surprisal: &surprisal,
            roles: &roles,
        },
        mappings,
    )
    .context("lowering the poem to a score")?;
    Ok((score, roles))
}

/// Prints how the piece measures: the interest metrics and the one-word edit test.
///
/// The edit test recompiles the poem once per word, through exactly the pipeline above, so what it
/// measures is what `prosody` itself would play.
fn report(
    text: &str,
    score: &Score,
    arguments: &Arguments,
    phonemiser: &impl Phonemiser,
    lexicon: &impl Lexicon,
) -> Result<()> {
    let sensitivity = sensitivity::measure(text, DEFAULT_PROBES, |edited| {
        let poem = Poem::parse(edited.to_owned()).ok()?;
        lower_poem(&poem, phonemiser, lexicon, &arguments.mappings)
            .ok()
            .map(|(score, _)| score)
    });
    emit(&Report::profiled(score, sensitivity, arguments.profile).to_text())
}

/// Sends every message of a schedule over UDP.
///
/// The whole schedule at once, with no waiting: sequencing in real time is the synthesis engine's
/// job, and a CLI that slept between messages would be a very bad clock. The onsets are in the
/// messages; `SuperCollider` can hold them.
fn send(schedule: &Schedule, address: &str) -> Result<()> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").context("opening a UDP socket")?;
    // The tempo first, so a receiver knows how long a beat is before any note arrives.
    socket
        .send_to(&schedule.tempo_message().encode(), address)
        .with_context(|| format!("sending to {address}"))?;
    for message in schedule.messages() {
        socket
            .send_to(&message.encode(), address)
            .with_context(|| format!("sending to {address}"))?;
    }
    eprintln!(
        "prosody: sent {} messages to {address} at {} bpm",
        schedule.messages().len(),
        schedule.beats_per_minute()
    );
    // A header key the schedule does not apply is said out loud, not dropped (`CLAUDE.md` §6).
    for (key, value) in schedule.unapplied() {
        eprintln!("prosody: {key} = {value} is not applied to an OSC schedule");
    }
    if schedule.unsent_chords() > 0 {
        eprintln!(
            "prosody: {} chords of the harmony layer are not sent: OSC has no chord message",
            schedule.unsent_chords()
        );
    }
    Ok(())
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

/// Writes to standard output, treating a closed pipe as success.
///
/// Rust ignores `SIGPIPE`, so `print!` *panics* on `EPIPE` — and `prosody poem.txt | head -1` is
/// the most ordinary thing a user does with a program whose whole job is text on stdout.
/// `CLAUDE.md` §6 forbids panicking on user input, and §9 notes that a panic in a live-reload loop
/// kills the session — which is no longer hypothetical now that there is one.
fn emit(text: &str) -> Result<()> {
    let mut out = std::io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => other.context("writing to standard output"),
    }
}

/// Prints the poem with what each token became written under it — `SPEC.md` §8's debug view.
///
/// The rendering lives in `prosody_core::annotate` so it can be tested without running a binary;
/// this adds what only the IO edge knows: the palette, and the mappings chosen away from their
/// defaults.
fn explain(
    poem: &Poem,
    score: &Score,
    style: &Style,
    roles: &PoemRoles,
    mappings: &Mappings,
) -> Result<()> {
    let mut out = String::new();
    let _ = writeln!(out, "style");
    let fields = style.describe();
    // Pad to the longest key, so a key added to the palette cannot push its value off the column.
    let width = fields.keys().map(|key| key.len()).max().unwrap_or(0);
    for (key, value) in fields {
        let _ = writeln!(out, "  {key:<width$} {value}");
    }
    // Every candidate mapping not at its default, so the view says which piece it describes.
    let changed: Vec<Choice> = mappings.changed().collect();
    if !changed.is_empty() {
        let _ = writeln!(out, "mappings");
        for choice in changed {
            let _ = writeln!(
                out,
                "  {:<width$} {}",
                choice.mapping().name(),
                choice.variant().name()
            );
        }
    }
    let _ = writeln!(
        out,
        "\n{} bars, {} voices, {} events",
        score.bars(),
        score.voices().len(),
        score.events().len()
    );
    let _ = writeln!(out);
    out.push_str(&annotate::render(poem, &annotate::run(poem, score, roles)));
    emit(&out)
}
