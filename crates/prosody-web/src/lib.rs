//! `prosody-web` — write a poem in a browser and hear it.
//!
//! `SPEC.md` §7.6 wants the cheapest possible audition path, and §8 wants the debug view alongside
//! it. A page with the poem on one side and both the pattern and the annotation on the other is
//! the cheapest that exists: type a word, hear the difference, and see which pass caused it.
//!
//! Every musical decision belongs to `prosody-core` and every palette decision to `prosody-emit`;
//! nothing here does more than move text. The page itself is a separate program (`web/`, built
//! with Vite); this library is what it asks for — its data, `/site.json`, and the compiled views —
//! with no IO at all, so the same answers come from two places: the local server (`src/main.rs`,
//! all the IO, which also serves the built page) and the page's own WebAssembly build
//! (`prosody-wasm`), which runs the whole compiler in the browser so the page can be hosted as
//! static files ([`Transport`]).
//!
//! The page also compares. Each candidate mapping ([`Mapping`]) is a selector, and either of two
//! settings, A and B, can choose any of its variants; the page plays one, shows both patterns, and
//! measures both with `prosody-score`, so what a choice changes can be heard and read side by side.

use std::fmt::Write as _;

use anyhow::{Context as _, Result};
use prosody_cmudict::CmuDict;
use prosody_core::annotate;
use prosody_core::lower::{self, Analyses};
use prosody_core::mapping::{Mapping, Mappings};
use prosody_core::voice::Score;
use prosody_core::{Fallback, Poem, meter, repetition, rhyme, role, sound, surprisal};
use prosody_emit::style::{
    BANKS, KITS, PRESETS as PALETTES, RIDE_FALLBACKS, SOUNDFONTS, SYNTHS, VOICINGS, preset_keys,
};
use prosody_emit::{Backend as _, Reading, Scale, Strudel, Style, reading, trace};
use prosody_lexicon::EnglishLexicon;
use prosody_lts::LetterToSound;
use prosody_score::{DEFAULT_PROBES, Profile, Report, sensitivity};

mod exchange;

pub use exchange::{Request, Response};

/// Where the page sends what it asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// To the local server, over HTTP.
    Server,
    /// To the compiler built as WebAssembly, loaded next to the page: the static build, which
    /// needs no server.
    Wasm,
}

impl Transport {
    /// Its name, as the page reads it from `/site.json`.
    const fn name(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Wasm => "wasm",
        }
    }
}

/// The page's data, and the answer to everything it asks.
#[derive(Debug, Clone)]
pub struct Site {
    data: String,
    presets: String,
}

impl Site {
    /// The site for `transport`, its data rendered once.
    ///
    /// # Errors
    ///
    /// If an example poem embedded as a preset no longer parses — which a test rules out.
    pub fn new(transport: Transport) -> Result<Self> {
        Ok(Self {
            data: site_json(transport)?,
            presets: presets_json()?,
        })
    }

    /// The page's data, `/site.json`: everything it needs to know before it can ask for anything.
    #[must_use]
    pub fn data(&self) -> &str {
        &self.data
    }

    /// Answers one request: the page's data, the presets, or one of the compiled views. The page
    /// itself and its modules are files, served by whoever serves the built page.
    ///
    /// `/presets.json` is the example poems as the preset menu has them — name, label, language,
    /// poem and palette — for the tool that records their readings (`scripts/readings/`), so that
    /// a preset's language has one source. Both JSON documents answer a `POST` as well as a `GET`
    /// because the WebAssembly build's binding only ever posts.
    #[must_use]
    pub fn respond(&self, request: &Request) -> Response {
        match (request.method.as_str(), request.path.as_str()) {
            ("GET" | "POST", "/site.json") => Response::json(self.data.clone()),
            ("GET" | "POST", "/presets.json") => Response::json(self.presets.clone()),
            ("POST", "/pattern") => compile(request, &Output::Pattern),
            ("POST", "/explain") => compile(request, &Output::Explain),
            ("POST", "/score") => compile(request, &Output::Score),
            ("POST", "/reading") => compile(request, &Output::Reading),
            ("POST", "/trace") => compile(request, &Output::Trace),
            _ => Response::problem(404, "no such thing here\n".to_owned()),
        }
    }
}

/// The page's data as one JSON object: the option lists, the mappings, the presets and palettes,
/// the scales' steps, the poem it opens with, and where it sends its requests.
///
/// From the tables themselves rather than typed into the page, so that the controls cannot drift
/// from what [`Style`] actually accepts — adding a scale in one place adds it to the page.
fn site_json(transport: Transport) -> Result<String> {
    let list = |values: &[&str]| {
        let quoted: Vec<String> = values.iter().map(|value| json_string(value)).collect();
        format!("[{}]", quoted.join(","))
    };
    let scales: Vec<&str> = Scale::ALL.iter().map(|scale| scale.name()).collect();
    // Each mapping with what it does, its variants and its default, in the order the core lists
    // them.
    let mappings: Vec<String> = Mapping::ALL
        .iter()
        .map(|mapping| {
            let variants: Vec<&str> = mapping.variants().iter().map(|v| v.name()).collect();
            format!(
                "{{\"name\":{},\"what\":{},\"variants\":{},\"default\":{}}}",
                json_string(mapping.name()),
                json_string(mapping.describe()),
                list(&variants),
                json_string(mapping.default_variant().name())
            )
        })
        .collect();

    let instruments: Vec<&str> = SYNTHS.iter().chain(SOUNDFONTS).copied().collect();
    let palettes: Vec<String> = PALETTES
        .iter()
        .filter_map(|name| {
            Style::preset(name)
                .map(|style| format!("{}:{}", json_string(name), palette_json(&style)))
        })
        .collect();
    // The page's control for the tempo window is two boxes, `low` and `high`.
    let palette_keys: Vec<String> = PALETTES
        .iter()
        .filter_map(|name| {
            let keys: Vec<&str> = preset_keys(name)?
                .into_iter()
                .flat_map(|key| match key {
                    "tempo_range" => vec!["low", "high"],
                    other => vec![other],
                })
                .collect();
            Some(format!("{}:{}", json_string(name), list(&keys)))
        })
        .collect();

    let fields = [
        ("transport", json_string(transport.name())),
        ("scales", list(&scales)),
        ("scaleSteps", scale_steps()),
        ("banks", list(BANKS)),
        ("instruments", list(&instruments)),
        ("kits", list(KITS)),
        ("voicings", list(VOICINGS)),
        ("rideFallbacks", list(RIDE_FALLBACKS)),
        ("mappings", format!("[{}]", mappings.join(","))),
        ("presets", presets_json()?),
        ("palettes", format!("{{{}}}", palettes.join(","))),
        ("paletteKeys", format!("{{{}}}", palette_keys.join(","))),
        ("styles", styles_json()),
        ("layers", layers_json()),
        ("seed", json_string(SEED)),
    ];
    let fields: Vec<String> = fields
        .iter()
        .map(|(key, value)| format!("{}:{value}", json_string(key)))
        .collect();
    Ok(format!("{{{}}}", fields.join(",")))
}

/// Every named palette as the page's style picker shows it: what it sounds like, in a few words,
/// and the mapping variants it suggests for side A — what its genre would have the poem *do*.
///
/// The suggestions are the page's and only the page's. A style is palette (I3): a header naming
/// `jazz` changes the sound and nothing else. A walking bass and cadences are mappings, and on the
/// page a writer chooses mappings; so picking a style there also sets these on A, visibly, in the
/// Mappings folder, where they can be set back — the user's decision of 2026-10-25, "suggest, don't
/// impose". In the order of [`PALETTES`], which a test checks, as it checks every name below.
const STYLES: &[StyleEntry] = &[
    StyleEntry {
        name: "default",
        what: "electric piano over an acoustic bass, a modest room",
        suggests: &[],
    },
    StyleEntry {
        name: "warm",
        what: "vibraphone over a fretless bass, a long room and echo",
        suggests: &[],
    },
    StyleEntry {
        name: "raw",
        what: "bare triangle waves, dry and on the grid",
        suggests: &[],
    },
    // A walking bass and the turn of open rhymes to their answers: PLAN.md's "structural half of
    // jazz", the two candidates the user approved to compare by ear.
    StyleEntry {
        name: "jazz",
        what: "a piano trio: Rhodes sevenths, upright bass, ride, swung",
        suggests: &[("bass", "syllables"), ("harmony", "cadence")],
    },
    // A chord a rhyme, looping as the rhymes do, and the small words dropped back: a beat tape.
    StyleEntry {
        name: "lofi",
        what: "worn electric piano, sevenths, the 808, a lazy swing",
        suggests: &[("harmony", "rhyme"), ("function-words", "light")],
    },
    // Four on the floor: the bass on every foot, every line squared to the grid, the small words
    // ghosted.
    StyleEntry {
        name: "techno",
        what: "synth bass and a sawtooth lead on the 909, straight",
        suggests: &[
            ("bass", "feet"),
            ("stack", "straight"),
            ("function-words", "light"),
        ],
    },
    // Bars that breathe rather than count, and a chord under each rhyme to float on.
    StyleEntry {
        name: "ambient",
        what: "celesta and a halo pad in a long, wet room",
        suggests: &[("pulse", "elastic"), ("harmony", "rhyme")],
    },
    // The bass on the beat, the guitar turning each open rhyme to its answer.
    StyleEntry {
        name: "bossa",
        what: "flute over nylon-guitar sevenths and an upright bass",
        suggests: &[("bass", "feet"), ("harmony", "cadence")],
    },
    // A walking continuo and cadences, as a trio sonata moves.
    StyleEntry {
        name: "chamber",
        what: "harpsichord, cello and strings, harmonic minor",
        suggests: &[("bass", "syllables"), ("harmony", "cadence")],
    },
    // A hymn's cadences, its bass on each foot, and each verse read through.
    StyleEntry {
        name: "hymn",
        what: "organ, choir and a double bass, in a church",
        suggests: &[
            ("harmony", "cadence"),
            ("bass", "feet"),
            ("reading", "stanza"),
        ],
    },
];

/// A style as the picker shows it: its palette's name, what it sounds like, and the mapping
/// variants it suggests for A, as `(mapping, variant)`.
struct StyleEntry {
    name: &'static str,
    what: &'static str,
    suggests: &'static [(&'static str, &'static str)],
}

/// The pattern's layers as JSON, `[{"name", "colour"}]`, in lane order (`strudel::layers`).
fn layers_json() -> String {
    let layers: Vec<String> = prosody_emit::strudel::layers()
        .iter()
        .map(|(name, colour)| {
            format!(
                "{{\"name\":{},\"colour\":{}}}",
                json_string(name),
                json_string(colour)
            )
        })
        .collect();
    format!("[{}]", layers.join(","))
}

/// [`STYLES`] as JSON for the page: `[{"name", "what", "suggests": {mapping: variant}}]`.
fn styles_json() -> String {
    let styles: Vec<String> = STYLES
        .iter()
        .map(
            |StyleEntry {
                 name,
                 what,
                 suggests,
             }| {
                let suggests: Vec<String> = suggests
                    .iter()
                    .map(|(mapping, variant)| {
                        format!("{}:{}", json_string(mapping), json_string(variant))
                    })
                    .collect();
                format!(
                    "{{\"name\":{},\"what\":{},\"suggests\":{{{}}}}}",
                    json_string(name),
                    json_string(what),
                    suggests.join(",")
                )
            },
        )
        .collect();
    format!("[{}]", styles.join(","))
}

/// Each scale's semitones above its root, by name, for the page's note circle — from [`Scale`]
/// itself, so a scale added there lights the right notes here.
fn scale_steps() -> String {
    let entries: Vec<String> = Scale::ALL
        .iter()
        .map(|scale| {
            let steps: Vec<String> = scale.steps().iter().map(u8::to_string).collect();
            format!("{}:[{}]", json_string(scale.name()), steps.join(","))
        })
        .collect();
    format!("{{{}}}", entries.join(","))
}

/// The example poems, for the preset menu: a file name, a label and the language the poem is in,
/// then the file itself.
///
/// Embedded rather than read at run time so the binary works from any directory; a test checks
/// that every file in `examples/` is here, so a new example cannot be missing from the menu.
///
/// The language is a BCP 47 tag, for the reading (`SPEC.md` §4.7): the page reads a preset in its
/// own language, with a voice that speaks it — Prešeren in Slovenian. It is not part of the
/// poem's header: the style is palette (I3), the reading is the page's, and the CLI, which never
/// reads aloud, has no use for it. Ball's *Karawane* is sound poetry with no language of its own;
/// it is read with English voices.
const PRESETS: &[(&str, &str, &str, &str)] = &[
    (
        "ball-karawane",
        "Ball — Karawane",
        "en",
        include_str!("../../../examples/ball-karawane.poem"),
    ),
    (
        "blake-tyger",
        "Blake — The Tyger",
        "en",
        include_str!("../../../examples/blake-tyger.poem"),
    ),
    (
        "carroll-jabberwocky",
        "Carroll — Jabberwocky",
        "en",
        include_str!("../../../examples/carroll-jabberwocky.poem"),
    ),
    (
        "crane-in-the-desert",
        "Crane — In the desert",
        "en",
        include_str!("../../../examples/crane-in-the-desert.poem"),
    ),
    (
        "dickinson-because",
        "Dickinson — Because I could not stop for Death",
        "en",
        include_str!("../../../examples/dickinson-because.poem"),
    ),
    (
        "dunbar-we-wear-the-mask",
        "Dunbar — We Wear the Mask",
        "en",
        include_str!("../../../examples/dunbar-we-wear-the-mask.poem"),
    ),
    (
        "hopkins-pied-beauty",
        "Hopkins — Pied Beauty",
        "en",
        include_str!("../../../examples/hopkins-pied-beauty.poem"),
    ),
    (
        "kjv-psalm-23",
        "Psalm 23 (King James)",
        "en",
        include_str!("../../../examples/kjv-psalm-23.poem"),
    ),
    (
        "lowell-night-clouds",
        "Lowell — Night Clouds",
        "en",
        include_str!("../../../examples/lowell-night-clouds.poem"),
    ),
    (
        "lowell-patterns",
        "Lowell — Patterns",
        "en",
        include_str!("../../../examples/lowell-patterns.poem"),
    ),
    (
        "naidu-bazaars-of-hyderabad",
        "Naidu — In the Bazaars of Hyderabad",
        "en",
        include_str!("../../../examples/naidu-bazaars-of-hyderabad.poem"),
    ),
    (
        "poe-annabel-lee",
        "Poe — Annabel Lee",
        "en",
        include_str!("../../../examples/poe-annabel-lee.poem"),
    ),
    (
        "preseren-zdravljica",
        "Prešeren — Zdravljica",
        "sl",
        include_str!("../../../examples/preseren-zdravljica.poem"),
    ),
    (
        "shakespeare-sonnet-18",
        "Shakespeare — Sonnet 18",
        "en",
        include_str!("../../../examples/shakespeare-sonnet-18.poem"),
    ),
    (
        "stein-tender-buttons",
        "Stein — Tender Buttons",
        "en",
        include_str!("../../../examples/stein-tender-buttons.poem"),
    ),
    (
        "tagore-gitanjali-35",
        "Tagore — Gitanjali 35",
        "en",
        include_str!("../../../examples/tagore-gitanjali-35.poem"),
    ),
    (
        "whitman-song-of-myself",
        "Whitman — Song of Myself",
        "en",
        include_str!("../../../examples/whitman-song-of-myself.poem"),
    ),
    (
        "wilde-theocritus",
        "Wilde — Theocritus",
        "en",
        include_str!("../../../examples/wilde-theocritus.poem"),
    ),
];

/// The presets as a JSON array for the page: each poem without its header, and the palette that
/// header asked for — the defaults when it has none — in the form the controls take.
///
/// The palette goes to the controls rather than staying in the poem because the controls are the
/// one source of truth for it (see [`body_of`]).
fn presets_json() -> Result<String> {
    let mut presets = Vec::new();
    for (name, label, language, text) in PRESETS {
        let poem = Poem::parse((*text).to_owned()).with_context(|| format!("preset {name}"))?;
        let style = Style::parse_optional(poem.frontmatter().and_then(|s| s.text(poem.source())))
            .with_context(|| format!("preset {name}'s palette"))?;
        presets.push(format!(
            "{{\"name\":{},\"label\":{},\"language\":{},\"poem\":{},\"palette\":{}}}",
            json_string(name),
            json_string(label),
            json_string(language),
            json_string(body_of(text)),
            palette_json(&style),
        ));
    }
    Ok(format!("[{}]", presets.join(",")))
}

/// A palette as the page's controls take it: one string per control, keyed by the control's id,
/// which is the header key — except the tempo window, which is two boxes, and the root, which the
/// menu spells `C#` where the debug view writes `cs`.
///
/// Built from [`Style::describe`], which writes every value in header units, so a field added to
/// the style reaches the page without this function changing.
fn palette_json(style: &Style) -> String {
    let fields: Vec<String> = style
        .describe()
        .into_iter()
        .flat_map(|(key, value)| match key {
            "tempo_range" => {
                let (low, high) = value.split_once("..").unwrap_or(("110", "130"));
                vec![
                    format!("\"low\":{}", json_string(low)),
                    format!("\"high\":{}", json_string(high)),
                ]
            }
            "root" => vec![format!("\"root\":{}", json_string(&root_label(&value)))],
            _ => vec![format!("{}:{}", json_string(key), json_string(&value))],
        })
        .collect();
    format!("{{{}}}", fields.join(","))
}

/// A root as the page's menu spells it: `cs` → `C#`.
fn root_label(name: &str) -> String {
    let sharp = name.ends_with('s');
    let letter = name.trim_end_matches('s').to_uppercase();
    if sharp { format!("{letter}#") } else { letter }
}

/// The poem the page opens with.
///
/// Blake, because its returning first stanza makes `SPEC.md` §4.3's refrain row audible within
/// four bars — the clearest single demonstration of the idea in the public domain.
const SEED: &str = "Tyger Tyger, burning bright,\n\
                    In the forests of the night;\n\
                    What immortal hand or eye,\n\
                    Could frame thy fearful symmetry?\n\
                    \n\
                    In what distant deeps or skies.\n\
                    Burnt the fire of thine eyes?\n\
                    On what wings dare he aspire?\n\
                    What the hand, dare seize the fire?\n";

/// A JSON string literal, for the page's data.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // `<` too, so the data stays safe to inline in an HTML `<script>` element.
            '<' => out.push_str("\\u003c"),
            other if (other as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", other as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Which of the views the page asked for.
enum Output {
    Pattern,
    Explain,
    /// The measurements, as JSON, so the page can set two settings side by side.
    Score,
    /// The reading's cues and cue pattern, as JSON (`prosody_emit::reading`): the page speaks
    /// them on Strudel's clock, when it is asked to.
    Reading,
    /// Which syllable sounds when, as JSON (`prosody_emit::trace`): the page lights the poem from
    /// it while the piece plays.
    Trace,
}

/// Compiles the request body under the palette in its query string.
///
/// The poem is the body and the palette is the query, so nothing has to encode a poem — no JSON,
/// no escaping, and a body that is exactly the bytes the writer typed.
fn compile(request: &Request, output: &Output) -> Response {
    let source = format!("{}{}", header(request), body_of(&request.body));
    let mappings = match mappings(request) {
        Ok(mappings) => mappings,
        Err(reason) => return Response::problem(400, format!("{reason}\n")),
    };
    let profile = match request.param("profile") {
        None => Profile::Machine,
        Some(name) => match Profile::named(name) {
            Some(profile) => profile,
            None => return Response::problem(400, format!("unknown profile `{name}`\n")),
        },
    };
    match render(&source, &request.body, output, &mappings, profile) {
        Ok(text) => Response::text(text),
        // A poem that will not compile is what the middle of a keystroke looks like, so the reason
        // goes back with a 400 and the page shows it rather than blanking.
        Err(error) => Response::problem(400, format!("{error:#}\n")),
    }
}

/// The palette keys the page sends that are names, written as TOML strings.
const NAMED_KEYS: &[&str] = &[
    "scale",
    "root",
    "bank",
    "bass",
    "melody",
    "kit",
    "voicing",
    "ride_fallback",
    "chord_sound",
];

/// The palette keys the page sends that are numbers, in header units.
const NUMBER_KEYS: &[&str] = &[
    "swing",
    "reverb",
    "room_size",
    "delay",
    "high_cut",
    "drive",
    "compress",
    "humanise",
    "drums_gain",
    "bass_gain",
    "melody_gain",
    "chord_gain",
    "density_level",
    "bass_octave",
    "melody_octave",
];

/// The style header the controls describe.
///
/// Only what the request names is written, so whatever it leaves out takes [`Style::default`]'s
/// value — the defaults live in one place, not here as well. Every value is validated by
/// [`Style::parse`] like any header's; this only quotes names and refuses numbers that are not
/// numbers, so a malformed one falls back rather than breaking the header around it.
fn header(request: &Request) -> String {
    let number = |name: &str, fallback: u16| {
        request
            .param(name)
            .and_then(|given| given.parse::<u16>().ok())
            .filter(|given| *given > 0)
            .unwrap_or(fallback)
    };
    let (low, high) = (number("low", 110), number("high", 130));
    // The two tempo boxes are independent, so the writer passes through states where the low one
    // is above the high one. Ordering them here beats refusing to compile mid-edit.
    let (low, high) = (low.min(high), low.max(high));

    let mut out = format!("+++\n[style]\ntempo_range = [{low}, {high}]\n");
    for key in NAMED_KEYS {
        if let Some(value) = request.param(key) {
            let _ = writeln!(out, "{key} = {}", json_string(value));
        }
    }
    for key in NUMBER_KEYS {
        if let Some(value) = request
            .param(key)
            .and_then(|given| given.parse::<f64>().ok())
            .filter(|given| given.is_finite())
        {
            let _ = writeln!(out, "{key} = {value}");
        }
    }
    out.push_str("+++\n");
    out
}

/// The mappings the request chooses: every default, then each `name:variant` in `set`
/// (comma-separated), then each name in `off` switched off. An unknown mapping or variant is an
/// error naming it, so a typo is refused rather than quietly comparing a setting with itself.
fn mappings(request: &Request) -> Result<Mappings, String> {
    let names = |param: &str| -> Vec<String> {
        request
            .param(param)
            .unwrap_or("")
            .split(',')
            .filter(|item| !item.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let mut chosen = Mappings::all();
    for item in names("set") {
        let (name, variant) = item
            .split_once(':')
            .ok_or_else(|| format!("`set` wants name:variant, not `{item}`"))?;
        let mapping = known(name)?;
        let choice = mapping.choose_named(variant).ok_or_else(|| {
            let variants: Vec<&str> = mapping.variants().iter().map(|v| v.name()).collect();
            format!(
                "unknown variant `{variant}` of mapping `{name}` (it has {})",
                variants.join(", ")
            )
        })?;
        chosen = chosen.with(choice);
    }
    for name in names("off") {
        chosen = chosen.without(known(&name)?);
    }
    Ok(chosen)
}

/// The mapping called `name`, or an error naming it.
fn known(name: &str) -> Result<Mapping, String> {
    Mapping::named(name).ok_or_else(|| format!("unknown mapping `{name}`"))
}

/// The poem without any header of its own.
///
/// Someone will paste a poem that already has one — one of the `examples/` files, most likely —
/// and the controls have to stay the single source of truth, or the page would show one palette
/// and play another.
fn body_of(text: &str) -> &str {
    let trimmed = text.trim_start();
    if !trimmed.starts_with("+++") {
        return text;
    }
    trimmed
        .get(3..)
        .and_then(|rest| rest.find("+++").map(|end| &rest[end + 3..]))
        .map_or(text, |body| body.trim_start_matches('\n'))
}

/// Compiles a poem, the way the CLI does. `source` is what compiles — the controls' header and
/// the poem — and `typed` the text box as the writer has it, which the trace's positions are in.
fn render(
    source: &str,
    typed: &str,
    output: &Output,
    mappings: &Mappings,
    profile: Profile,
) -> Result<String> {
    let poem = Poem::parse(source.to_owned()).context("parsing the poem")?;
    let style = Style::parse_optional(poem.frontmatter().and_then(|s| s.text(poem.source())))
        .context("reading the palette")?;
    let phonemiser =
        Fallback::new(CmuDict::new(), LetterToSound::new()).context("preparing the phonemiser")?;
    let lexicon = EnglishLexicon::new();
    let (score, roles) = lower_poem(&poem, &phonemiser, &lexicon, mappings)?;

    match output {
        Output::Pattern => Strudel::new()
            .render(&score, &style)
            .map_err(|error| anyhow::anyhow!("{error}"))
            .context("rendering the pattern"),
        // The annotation is worth having even when there is nothing to play: a poem of entirely
        // unpronounceable words is exactly when a writer most wants to see why.
        Output::Explain => {
            let mut out = String::new();
            let fields = style.describe();
            // Pad to the longest key, so a key added to the palette cannot push its value off
            // the column.
            let width = fields.keys().map(|key| key.len()).max().unwrap_or(0);
            for (key, value) in fields {
                let _ = writeln!(out, "{key:<width$} {value}");
            }
            // Over a chord lane the melody is single notes whatever `voicing` says (`SPEC.md`
            // §4.6), so the view says what is heard.
            if let Some(words) = prosody_emit::melody_over_harmony(&score) {
                let _ = writeln!(out, "{:<width$} {words}", "rhyme notes");
            }
            // Every choice that is not the default, so the view says which piece it describes.
            let changed: Vec<String> = mappings
                .changed()
                .map(|choice| format!("{}={}", choice.mapping().name(), choice.variant().name()))
                .collect();
            if !changed.is_empty() {
                let _ = writeln!(out, "{:<width$} {}", "mappings", changed.join(", "));
            }
            let _ = writeln!(out);
            out.push_str(&annotate::render(
                &poem,
                &annotate::run(&poem, &score, &roles),
            ));
            Ok(out)
        }
        Output::Score => {
            // The edit test recompiles the poem once per word under the same mappings, so it
            // measures the piece being heard, not the full one.
            let sensitivity = sensitivity::measure(source, DEFAULT_PROBES, |edited| {
                let poem = Poem::parse(edited.to_owned()).ok()?;
                lower_poem(&poem, &phonemiser, &lexicon, mappings)
                    .ok()
                    .map(|(score, _)| score)
            });
            Ok(report_json(&Report::profiled(&score, sensitivity, profile)))
        }
        Output::Reading => reading::plan(&poem, &score, &style, mappings)
            .map(|reading| reading_json(&reading, source, typed))
            .context("planning the reading"),
        Output::Trace => Ok(trace_json(&trace::plan(&score, &style), source, typed)),
    }
}

/// A reading as JSON, for the page to speak at either pace (`SPEC.md` §4.7).
///
/// - `pattern` and `cues`: the fitted pace — the cue pattern to evaluate, and each cue, the line
///   it speaks in its slot at its fitted `rate`.
/// - `natural` and `lines`: the natural pace — its cue pattern, whose cues are the earliest bars
///   lines may begin in, and every line it reads in reading order with its `onset` (`breath`,
///   `at` with its `slot`, or `follows`).
/// - every spoken line, at either pace, carries its `stanza`, its `pace` (the speed the natural
///   pace asks a voice for), the `pause` after it in seconds and what it `ends` on, and its
///   `words`, each with the syllables it sounds, for the page's word marker.
/// - `bars` (the cycle each slot begins on, then the piece's length) and `cpm`: the music's clock,
///   for the page to find a bar on the audio clock.
/// - `fill`: the share of its bar a fitted line is fitted to fill ([`reading::FILL`]), so the page
///   can fit a neural voice's rendered line to the same room.
/// - `unread` and `natural_unread`: each line left unread, and why, for the page to say.
///
/// `start` and `end` are bytes in the source, for the log; `from` and `to` are where a line or a
/// word is in the text box, for the page to show it (see [`text_box`]).
fn reading_json(reading: &Reading, source: &str, typed: &str) -> String {
    let at = text_box(source, typed);
    let place =
        |span: prosody_core::span::Span| at(span.start()).zip(at(span.end())).unwrap_or((0, 0));
    let spoken = |line: &reading::Spoken| {
        let (from, to) = place(line.span());
        let words: Vec<String> = line
            .words()
            .iter()
            .map(|word| {
                let (from, to) = place(word.span());
                format!(
                    "{{\"from\":{from},\"to\":{to},\"syllables\":{}}}",
                    word.syllables()
                )
            })
            .collect();
        format!(
            "\"line\":{},\"stanza\":{},\"start\":{},\"end\":{},\"from\":{from},\"to\":{to},\"syllables\":{},\"pace\":{},\"pause\":{},\"ends\":{},\"words\":[{}],\"text\":{}",
            line.line(),
            line.stanza(),
            line.span().start(),
            line.span().end(),
            line.syllables(),
            prosody_emit::fmt_param(line.pace().speed()),
            prosody_emit::fmt_param(line.pause().seconds()),
            json_string(line.pause().says()),
            words.join(","),
            json_string(line.text()),
        )
    };
    let cues: Vec<String> = reading
        .cues()
        .iter()
        .map(|cue| {
            format!(
                "{{\"slot\":{},\"rate\":{},{}}}",
                cue.slot(),
                prosody_emit::fmt_param(cue.rate()),
                spoken(cue.spoken()),
            )
        })
        .collect();
    let lines: Vec<String> = reading
        .natural()
        .iter()
        .map(|natural| {
            let slot = natural
                .onset()
                .slot()
                .map_or_else(|| "null".to_owned(), |slot| slot.to_string());
            format!(
                "{{\"onset\":{},\"slot\":{slot},{}}}",
                json_string(natural.onset().name()),
                spoken(natural.spoken()),
            )
        })
        .collect();
    // The lines the reading leaves out, so the page can say so (CLAUDE.md §6: a loss is visible).
    let unread = |unread: &[reading::Unread]| -> String {
        unread
            .iter()
            .map(|line| {
                format!(
                    "{{\"line\":{},\"start\":{},\"end\":{},\"why\":{},\"text\":{}}}",
                    line.line(),
                    line.span().start(),
                    line.span().end(),
                    json_string(line.why().says()),
                    json_string(line.text()),
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    let bars: Vec<String> = reading.bars().iter().map(ToString::to_string).collect();
    format!(
        "{{\"pattern\":{},\"natural\":{},\"bars\":[{}],\"cpm\":{},\"fill\":{},\"cues\":[{}],\"lines\":[{}],\"unread\":[{}],\"natural_unread\":[{}]}}\n",
        json_string(&reading.pattern()),
        json_string(&reading.natural_pattern()),
        bars.join(","),
        prosody_emit::fmt_param(reading.cpm()),
        prosody_emit::fmt_param(reading::FILL),
        cues.join(","),
        lines.join(","),
        unread(reading.unread()),
        unread(reading.natural_unread()),
    )
}

/// Where a byte of the source is in the text box the writer typed.
///
/// The text box is not the source: the source is the controls' header followed by the poem with
/// any pasted header of its own removed ([`body_of`]). So a byte is moved back onto the text box,
/// and counted in UTF-16 units, which is what a JavaScript string is indexed by — a poem with an
/// em dash in it would otherwise light the wrong letters from there on. `None` for a byte inside
/// the generated header, which has no words.
fn text_box<'a>(source: &str, typed: &'a str) -> impl Fn(usize) -> Option<usize> + 'a {
    let body = body_of(typed);
    let header = source.len().saturating_sub(body.len());
    let skipped = typed.len().saturating_sub(body.len());
    move |byte: usize| {
        let byte = byte.checked_sub(header)? + skipped;
        typed
            .get(..byte)
            .map(|before| before.encode_utf16().count())
    }
}

/// A trace as JSON: the pattern to query, and each mark — where its syllable is in the text box
/// ([`text_box`]), its colour, and whether its line is new here or looping.
fn trace_json(trace: &prosody_emit::Trace, source: &str, typed: &str) -> String {
    let at = text_box(source, typed);
    let marks: Vec<String> = trace
        .marks()
        .iter()
        .map(|mark| {
            // A mark inside the generated header cannot happen — the header has no syllables — so
            // a position that does not map is shown as nothing rather than guessed at.
            let (start, end) = at(mark.span().start())
                .zip(at(mark.span().end()))
                .unwrap_or((0, 0));
            format!(
                "{{\"start\":{start},\"end\":{end},\"colour\":{},\"fresh\":{},\"melody\":{},\"bass\":{}}}",
                json_string(mark.colour()),
                mark.fresh(),
                mark.melody(),
                mark.bass(),
            )
        })
        .collect();
    // Where each bar begins, in the pattern's cycles, and the piece's length last; with the
    // cycles a minute, the page draws the roll's bar lines where the music's bars change — under
    // the steady pulse they are not evenly spaced.
    let bars: Vec<String> = trace.bars().iter().map(ToString::to_string).collect();
    format!(
        "{{\"pattern\":{},\"bars\":[{}],\"cpm\":{},\"marks\":[{}]}}\n",
        json_string(trace.pattern()),
        bars.join(","),
        prosody_emit::fmt_param(trace.cycles_per_minute()),
        marks.join(",")
    )
}

/// Every analysis pass and the lowering, under `mappings`.
fn lower_poem(
    poem: &Poem,
    phonemiser: &impl prosody_core::Phonemiser,
    lexicon: &impl prosody_core::Lexicon,
    mappings: &Mappings,
) -> Result<(Score, prosody_core::PoemRoles)> {
    let meter = meter::scan(poem, phonemiser);
    let rhyme = rhyme::analyse_with(poem, phonemiser, mappings.get(Mapping::Rhyme));
    let repetition = repetition::analyse(poem);
    let sound = sound::analyse(poem, phonemiser);
    let surprisal = surprisal::analyse(poem, phonemiser, lexicon);
    let roles = role::assign(&meter, &repetition).context("assigning roles")?;
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
    .context("lowering the poem")?;
    Ok((score, roles))
}

/// A report as JSON: each metric with its band and fit, then the totals. Numbers to three
/// decimals, as `Report::to_text` prints them, so the page shows what the CLI would.
fn report_json(report: &Report) -> String {
    let number = |value: f64| format!("{value:.3}");
    let metrics: Vec<String> = report
        .metrics()
        .iter()
        .map(|metric| {
            format!(
                "{{\"name\":{},\"value\":{},\"low\":{},\"high\":{},\"fit\":{}}}",
                json_string(metric.name()),
                number(metric.value()),
                number(metric.band().low()),
                number(metric.band().high()),
                number(metric.fit()),
            )
        })
        .collect();
    let legibility = report
        .sensitivity()
        .map_or_else(|| "null".to_owned(), |s| number(s.legibility()));
    format!(
        "{{\"metrics\":[{}],\"interest\":{},\"legibility\":{legibility},\"overall\":{}}}\n",
        metrics.join(","),
        number(report.interest()),
        number(report.overall()),
    )
}

// Tests may unwrap and index freely; a panic in a test is a test failure. See CLAUDE.md §3.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use prosody_core::mapping::Variant;

    use super::{Mapping, Mappings, Output, Profile, SEED, body_of, json_string, site_json};

    #[test]
    fn the_site_data_lists_every_palette_value_the_style_accepts() {
        // From `Style`'s own tables rather than typed into the page, so a scale added in one place
        // cannot be missing from the other.
        let data = site_json(super::Transport::Server).unwrap();
        for scale in prosody_emit::Scale::ALL {
            assert!(data.contains(scale.name()), "{} missing", scale.name());
        }
        for name in prosody_emit::style::BANKS
            .iter()
            .chain(prosody_emit::style::SYNTHS)
            .chain(prosody_emit::style::SOUNDFONTS)
        {
            assert!(data.contains(&format!("\"{name}\"")), "{name} missing");
        }
        // Every named palette, with its values, for the sound menu.
        for name in prosody_emit::style::PRESETS {
            assert!(
                data.contains(&format!("\"{name}\":{{")),
                "palette {name} missing"
            );
        }
        for name in prosody_emit::style::KITS
            .iter()
            .chain(prosody_emit::style::VOICINGS)
            .chain(prosody_emit::style::RIDE_FALLBACKS)
        {
            assert!(data.contains(&format!("\"{name}\"")), "{name} missing");
        }
        // The jazz palette tells the menu it sets the scale, swing and bank as well; warm does not.
        assert!(
            data.contains("\"jazz\":[\"bass\",\"melody\",\"kit\",\"ride_fallback\",\"voicing\""),
            "jazz's keys"
        );
        let jazz_keys = data.split("\"jazz\":[").nth(1).unwrap();
        let jazz_keys = &jazz_keys[..jazz_keys.find(']').unwrap()];
        assert!(
            jazz_keys.contains("\"scale\",\"swing\",\"bank\""),
            "{jazz_keys}"
        );
        let warm_keys = data.split("\"warm\":[").nth(1).unwrap();
        assert!(!warm_keys[..warm_keys.find(']').unwrap()].contains("scale"));
        // And the poem it opens with.
        assert!(data.contains(&format!("\"seed\":{}", json_string(SEED))));
    }

    #[test]
    fn a_pasted_header_is_replaced_by_the_controls() {
        // Otherwise the page would show one palette and play another.
        let pasted = "+++\n[style]\nscale = \"lydian\"\n+++\nthe fire will burn\n";
        assert_eq!(body_of(pasted), "the fire will burn\n");
        assert_eq!(body_of("the fire will burn\n"), "the fire will burn\n");
        assert_eq!(body_of(""), "");
        // A half-typed header is left alone rather than eating the poem.
        assert_eq!(
            body_of("+++\nscale = \"lydian\"\n"),
            "+++\nscale = \"lydian\"\n"
        );
    }

    #[test]
    fn every_style_is_a_palette_and_suggests_only_mappings_the_core_has() {
        // One entry per named palette, in their order, so the picker cannot miss one.
        let names: Vec<&str> = super::STYLES.iter().map(|style| style.name).collect();
        assert_eq!(names, prosody_emit::style::PRESETS.to_vec());
        for super::StyleEntry { name, suggests, .. } in super::STYLES {
            for (mapping, variant) in *suggests {
                let known = Mapping::ALL.iter().find(|m| m.name() == *mapping);
                assert!(known.is_some(), "{name}: no mapping `{mapping}`");
                let variants: Vec<&str> =
                    known.unwrap().variants().iter().map(|v| v.name()).collect();
                assert!(
                    variants.contains(variant),
                    "{name}: `{mapping}` has no `{variant}`"
                );
            }
        }
        // As the page reads it: jazz walks the bass and turns its rhymes; default suggests nothing.
        let data = site_json(super::Transport::Server).unwrap();
        assert!(data.contains(
            "{\"name\":\"jazz\",\"what\":\"a piano trio: Rhodes sevenths, upright bass, ride, swung\",\"suggests\":{\"bass\":\"syllables\",\"harmony\":\"cadence\"}}"
        ));
        assert!(
            data.contains("\"suggests\":{}}"),
            "default suggests nothing"
        );
    }

    #[test]
    fn the_site_data_names_each_layer_by_the_colour_its_events_carry() {
        // The colours the roll and the lit poem have always drawn each layer in (styles.css).
        let data = site_json(super::Transport::Server).unwrap();
        assert!(data.contains(
            "\"layers\":[{\"name\":\"pulse\",\"colour\":\"#c9a6ff\"},\
             {\"name\":\"ornament\",\"colour\":\"#6fd6c5\"},\
             {\"name\":\"bass\",\"colour\":\"#ffb86b\"},\
             {\"name\":\"rhyme\",\"colour\":\"#ff7aa8\"},\
             {\"name\":\"chords\",\"colour\":\"#8fb3ff\"}]"
        ));
        // And the pattern carries only those: every `.color` in a compiled poem is one of them.
        let pattern = super::compile(
            &request(&[("set", "harmony:cadence")], SEED),
            &Output::Pattern,
        )
        .body;
        assert_eq!(
            pattern.matches(".color(\"").count(),
            prosody_emit::strudel::layers()
                .iter()
                .map(|(_, colour)| pattern.matches(&format!(".color(\"{colour}\")")).count())
                .sum::<usize>()
        );
    }

    /// A JSON string field's value, unescaped: enough of JSON for the views this crate writes.
    fn json_field(json: &str, key: &str) -> String {
        let start = json.find(&format!("\"{key}\":\"")).unwrap() + key.len() + 4;
        let mut out = String::new();
        let mut chars = json[start..].chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return out,
                '\\' => match chars.next().unwrap() {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let code: String = chars.by_ref().take(4).collect();
                        out.push(char::from_u32(u32::from_str_radix(&code, 16).unwrap()).unwrap());
                    }
                    other => out.push(other),
                },
                other => out.push(other),
            }
        }
        out
    }

    #[test]
    fn no_poem_text_reaches_the_code_the_page_runs() {
        // A shared link hands a stranger's poem to the page, which evaluates what it compiles to —
        // the pattern, the reading's two cue patterns and the trace — as JavaScript. So no text of
        // the poem may reach that code except inside a `//` comment it cannot leave: no line of the
        // code outside a comment carries the poem's words, and no line terminator JavaScript knows
        // (U+2028, U+2029, a carriage return) appears at all, since one inside a comment would end
        // it. Every word below carries the marker `qzx`.
        let hostile = "Qzx \"qzx\"); globalThis.qzx = 1; //\n\
                       qzx` ${qzx} */ </script> 'qzx' \\qzx\\\n\
                       qzx\u{2028}qzx = 2;\u{2029}qzx = 3;\r\n\
                       the fire will qzx the night\n\
                       and qzx the dark to light\n\
                       \n\
                       the fire will qzx the night\n";
        let runs = |set: &str| {
            let query = [("set", set)];
            let pattern = super::compile(&request(&query, hostile), &Output::Pattern);
            assert_eq!(pattern.status, 200, "{}", pattern.body);
            let reading = super::compile(&request(&query, hostile), &Output::Reading).body;
            let trace = super::compile(&request(&query, hostile), &Output::Trace).body;
            vec![
                ("pattern", pattern.body),
                ("fitted cues", json_field(&reading, "pattern")),
                ("natural cues", json_field(&reading, "natural")),
                ("trace", json_field(&trace, "pattern")),
            ]
        };
        for set in [
            "",
            "harmony:both,bass:syllables,reading:arrives,pulse:elastic",
        ] {
            for (name, code) in runs(set) {
                assert!(code.len() > 40, "{name} is empty: {code}");
                for line in code.lines() {
                    let comment = line.trim_start().starts_with("//");
                    assert!(
                        comment || !line.to_lowercase().contains("qzx"),
                        "{name} ({set}): poem text outside a comment: {line}"
                    );
                }
                for terminator in ['\u{2028}', '\u{2029}', '\r'] {
                    assert!(!code.contains(terminator), "{name} ({set}): {terminator:?}");
                }
            }
        }
        // The controls cannot carry code either: a name a palette does not have is refused.
        for (key, value) in [
            ("bank", "tr909\")\n;globalThis.qzx = 1;//"),
            ("melody", "gm_piano\")"),
        ] {
            let refused = super::compile(&request(&[(key, value)], SEED), &Output::Pattern);
            assert_eq!(refused.status, 400, "{key}: {}", refused.body);
        }
    }

    #[test]
    fn the_site_data_says_where_to_send_requests() {
        // Served, the page asks the server; built as static files, the compiler beside it.
        let served = site_json(super::Transport::Server).unwrap();
        let static_ = site_json(super::Transport::Wasm).unwrap();
        assert!(
            served.starts_with("{\"transport\":\"server\","),
            "{served:.40}"
        );
        assert!(
            static_.starts_with("{\"transport\":\"wasm\","),
            "{static_:.40}"
        );
    }

    #[test]
    fn a_preset_is_read_in_its_own_language() {
        // Prešeren is Slovenian; every other example is read in English (Ball's sound poem has no
        // language of its own).
        let presets = super::presets_json().unwrap();
        assert!(
            presets.contains(
                "{\"name\":\"preseren-zdravljica\",\"label\":\"Prešeren — Zdravljica\",\"language\":\"sl\","
            ),
            "{presets}"
        );
        assert_eq!(presets.matches("\"language\":\"en\"").count(), 17);
        assert_eq!(
            presets.matches("\"language\":").count(),
            super::PRESETS.len()
        );
    }

    #[test]
    fn the_site_answers_its_data_and_views() {
        let site = super::Site::new(super::Transport::Server).unwrap();
        let get = |target: &str| site.respond(&super::Request::new("GET", target, String::new()));
        let data = get("/site.json");
        assert_eq!(data.status, 200);
        assert_eq!(data.content_type, "application/json; charset=utf-8");
        assert_eq!(data.body, site.data());
        // The WebAssembly binding only posts.
        let posted = site.respond(&super::Request::new("POST", "/site.json", String::new()));
        assert_eq!(posted.body, site.data());
        // The page itself is a file, served by whoever serves the built page.
        assert_eq!(get("/").status, 404);
        assert_eq!(get("/pattern").status, 404, "a view is a POST");
        let pattern = site.respond(&super::Request::new("POST", "/pattern", SEED.to_owned()));
        assert_eq!(pattern.status, 200);
        assert!(
            pattern
                .body
                .starts_with("// compiled from a poem by prosody\n")
        );
    }

    #[test]
    fn the_note_circle_lights_each_scales_own_notes() {
        // Phrygian dominant is 0 1 4 5 7 8 10.
        let data = site_json(super::Transport::Wasm).unwrap();
        assert!(data.contains("\"phrygian_dominant\":[0,1,4,5,7,8,10]"));
    }

    #[test]
    fn json_strings_are_escaped() {
        // Quotes, backslashes and control characters, and `<`, so the data can be inlined in a
        // `<script>` element without `</script>` in a poem ending it early.
        let escaped = json_string("a </script> b \"c\" \\d\ne");
        assert!(!escaped.contains("</script>"), "{escaped}");
        assert!(escaped.contains("\\u003c"), "{escaped}");
        assert!(escaped.contains("\\\"c\\\""), "{escaped}");
        assert!(escaped.contains("\\\\d"), "{escaped}");
        assert!(escaped.contains("\\n"), "{escaped}");
    }

    fn request(query: &[(&str, &str)], body: &str) -> super::Request {
        super::Request {
            method: "POST".to_owned(),
            path: "/".to_owned(),
            query: query
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            body: body.to_owned(),
        }
    }

    #[test]
    fn the_site_data_lists_every_mapping_the_core_can_leave_out() {
        let data = site_json(super::Transport::Server).unwrap();
        for mapping in Mapping::ALL {
            assert!(
                data.contains(&json_string(mapping.describe())),
                "{mapping:?} missing"
            );
        }
        // Each with its variants and its default, for the A/B selectors.
        for (name, selector) in [
            (
                "bass",
                "[\"off\",\"height\",\"syllables\",\"feet\"],\"default\":\"height\"}",
            ),
            (
                "reading",
                "[\"off\",\"arrives\",\"stanza\",\"breath\"],\"default\":\"breath\"}",
            ),
            ("pulse", "[\"elastic\",\"steady\"],\"default\":\"steady\"}"),
            (
                "stack",
                "[\"crossing\",\"straight\"],\"default\":\"crossing\"}",
            ),
            (
                "function-words",
                "[\"full\",\"light\"],\"default\":\"full\"}",
            ),
            (
                "long-stanzas",
                "[\"gradual\",\"erode\"],\"default\":\"erode\"}",
            ),
            ("drums", "[\"flat\",\"articulated\"],\"default\":\"flat\"}"),
            ("rhyme", "[\"strict\",\"slant\"],\"default\":\"strict\"}"),
            ("anchor", "[\"off\",\"on\"],\"default\":\"off\"}"),
            ("melody", "[\"rhyme\",\"assonance\"],\"default\":\"rhyme\"}"),
        ] {
            let at = data.find(&format!("{{\"name\":\"{name}\""));
            assert!(at.is_some(), "{name} missing");
            let at = at.unwrap();
            let entry = &data[at..];
            let entry = &entry[..=entry.find('}').unwrap()];
            assert!(
                entry.ends_with(&format!("\"variants\":{selector}")),
                "{entry}"
            );
        }
    }

    #[test]
    fn off_names_the_mappings_a_setting_leaves_out() {
        let all = super::mappings(&request(&[], "")).unwrap();
        assert_eq!(all, Mappings::all());
        let some = super::mappings(&request(&[("off", "bass,outro")], "")).unwrap();
        assert_eq!(
            some,
            Mappings::all()
                .without(Mapping::Bass)
                .without(Mapping::Outro)
        );
        // A typo is refused rather than quietly comparing a setting with itself.
        assert_eq!(
            super::mappings(&request(&[("off", "bass,polka")], "")),
            Err("unknown mapping `polka`".to_owned())
        );
        let refused = super::compile(&request(&[("off", "polka")], SEED), &Output::Pattern);
        assert_eq!(refused.status, 400);
        assert!(refused.body.contains("`polka`"), "{}", refused.body);
    }

    #[test]
    fn set_chooses_a_variant_per_mapping() {
        let chosen =
            super::mappings(&request(&[("set", "reading:stanza,bass:off,outro:on")], "")).unwrap();
        assert_eq!(
            chosen,
            Mappings::all()
                .with(Mapping::Reading.choose(Variant::Stanza).unwrap())
                .without(Mapping::Bass)
        );
        // `off` still works beside it, and wins: it is read last.
        let both = super::mappings(&request(
            &[("set", "reading:stanza"), ("off", "reading")],
            "",
        ))
        .unwrap();
        assert_eq!(both.get(Mapping::Reading), Variant::Off);
        // Unknown names and variants are refused, naming what was wrong.
        assert_eq!(
            super::mappings(&request(&[("set", "polka:on")], "")),
            Err("unknown mapping `polka`".to_owned())
        );
        assert_eq!(
            super::mappings(&request(&[("set", "bass:stanza")], "")),
            Err(
                "unknown variant `stanza` of mapping `bass` (it has off, height, syllables, feet)"
                    .to_owned()
            )
        );
        assert!(super::mappings(&request(&[("set", "reading")], "")).is_err());
        let refused = super::compile(&request(&[("set", "reading:loud")], SEED), &Output::Reading);
        assert_eq!(refused.status, 400);
        assert!(refused.body.contains("`loud`"), "{}", refused.body);
    }

    #[test]
    fn the_reading_follows_its_variant() {
        // The seed's second stanza transforms the first's loop: four stated bars (slots 0–3),
        // then four passes of four (SPEC.md §4.6). Pass 1 is |B1 A2 A3 A4| in slots 4–7, pass 2
        // |B1 B2 A3 A4| in 8–11, and so on, so under `arrives` (0-based) line 4 is read in slot
        // 4, line 5 in slot 9, line 6 in 14 and line 7 in 19 — five bars apart. Under `stanza`
        // lines 4–7 are read in slots 4, 5, 6, 7, while the loop is still turning into them.
        // Under `breath`, the default, stanza 1 is read in slots 0–3 and stanza 2 arrives in slot
        // 4 — the breath after slot 3 — so it starts at max(4, 3 + 2) = 5: slots 5, 6, 7, 8.
        let slots_of = |body: &str| -> Vec<(usize, usize)> {
            body.split("{\"slot\":")
                .skip(1)
                .map(|rest| {
                    let slot = rest.split(',').next().unwrap().parse().unwrap();
                    let line = rest.split("\"line\":").nth(1).unwrap();
                    (slot, line.split(',').next().unwrap().parse().unwrap())
                })
                .collect()
        };
        let breath = super::compile(&request(&[], SEED), &Output::Reading);
        let arrives = super::compile(
            &request(&[("set", "reading:arrives")], SEED),
            &Output::Reading,
        );
        let stanza = super::compile(
            &request(&[("set", "reading:stanza")], SEED),
            &Output::Reading,
        );
        let off = super::compile(&request(&[("set", "reading:off")], SEED), &Output::Reading);
        assert_eq!(
            slots_of(&arrives.body),
            vec![
                (0, 0),
                (1, 1),
                (2, 2),
                (3, 3),
                (4, 4),
                (9, 5),
                (14, 6),
                (19, 7)
            ]
        );
        assert_eq!(
            slots_of(&stanza.body),
            (0..8).map(|n| (n, n)).collect::<Vec<_>>()
        );
        assert_eq!(
            slots_of(&breath.body),
            vec![
                (0, 0),
                (1, 1),
                (2, 2),
                (3, 3),
                (5, 4),
                (6, 5),
                (7, 6),
                (8, 7)
            ]
        );
        assert!(breath.body.contains("reading = breath"), "{}", breath.body);
        assert_eq!(
            breath.body,
            super::compile(
                &request(&[("set", "reading:breath")], SEED),
                &Output::Reading
            )
            .body
        );
        assert!(off.body.contains("\"cues\":[]"), "{}", off.body);
        // The pattern and the explain view say which reading this is.
        assert!(stanza.body.contains("reading = stanza"), "{}", stanza.body);
        let explain = super::compile(
            &request(&[("set", "reading:stanza")], SEED),
            &Output::Explain,
        );
        assert!(
            explain.body.contains("mappings      reading=stanza"),
            "{}",
            explain.body
        );
        let plain = super::compile(&request(&[], SEED), &Output::Explain);
        assert!(!plain.body.contains("mappings  "), "{}", plain.body);
        // The reading as it shipped is now a choice away from the default, and is listed.
        let shipped = super::compile(
            &request(&[("set", "reading:arrives")], SEED),
            &Output::Explain,
        );
        assert!(
            shipped.body.contains("mappings      reading=arrives"),
            "{}",
            shipped.body
        );
        // The music does not move: the reading is decided after the lowering.
        let music =
            |query: &[(&str, &str)]| super::compile(&request(query, SEED), &Output::Pattern).body;
        assert_eq!(music(&[]), music(&[("set", "reading:off")]));
        assert_eq!(music(&[]), music(&[("set", "reading:arrives")]));
    }

    #[test]
    fn the_harmony_is_a_selector_that_adds_a_chord_lane() {
        // On the page: four variants, off by default.
        let data = site_json(super::Transport::Server).unwrap();
        assert!(
            data.contains(
                "\"variants\":[\"off\",\"rhyme\",\"cadence\",\"both\"],\"default\":\"off\"}"
            ),
            "the harmony selector"
        );
        // Off — the default — the pattern has no chord lane.
        let music =
            |query: &[(&str, &str)]| super::compile(&request(query, SEED), &Output::Pattern);
        assert!(!music(&[]).body.contains("the chords"));
        for variant in ["rhyme", "cadence", "both"] {
            let set = format!("harmony:{variant}");
            let response = music(&[("set", &set), ("chord_sound", "gm_vibraphone")]);
            assert_eq!(response.status, 200, "{}", response.body);
            assert!(response.body.contains("// the chords"), "{}", response.body);
            assert!(
                response.body.contains(".sound(\"gm_vibraphone\")"),
                "{variant}"
            );
            assert!(response.body.contains("//   chords by line: "), "{variant}");
            // The explain view names the choice and gives every line's harmony.
            let explain = super::compile(&request(&[("set", &set)], SEED), &Output::Explain);
            assert!(
                explain
                    .body
                    .contains(&format!("mappings      harmony={variant}")),
                "{}",
                explain.body
            );
            assert!(
                explain.body.contains("  harmony     §4.6"),
                "{}",
                explain.body
            );
            assert!(explain.body.contains("\n  harmony "), "{}", explain.body);
        }
        let wrong = music(&[("set", "harmony:polka")]);
        assert_eq!(wrong.status, 400, "{}", wrong.body);
    }

    #[test]
    fn the_explain_views_palette_values_share_one_column() {
        // The longest palette keys are 13 characters (density_level, melody_octave,
        // ride_fallback), so every value starts at byte 14: thirteen for the key, one space.
        let explain = super::compile(
            &request(&[("set", "reading:stanza"), ("density_level", "0.7")], SEED),
            &Output::Explain,
        );
        assert_eq!(explain.status, 200, "{}", explain.body);
        let header: Vec<&str> = explain.body.lines().take_while(|l| !l.is_empty()).collect();
        assert!(header.len() > 10, "{}", explain.body);
        for line in header {
            let (key, value) = line.split_at(14);
            assert!(
                key.ends_with(' ') && !value.starts_with(' ') && !value.is_empty(),
                "{line:?} is off the column"
            );
        }
        assert!(
            explain.body.contains("density_level 0.70"),
            "{}",
            explain.body
        );
        assert!(explain.body.contains("bass_octave   2"), "{}", explain.body);
    }

    #[test]
    fn leaving_the_outro_out_shortens_the_piece() {
        // The seed's two stanzas share a layer: stanza 1 is stated in four bars, then stanza 2
        // transforms it in four passes of four — twenty — and the outro repeats stanza 2's four.
        let full = super::compile(&request(&[], SEED), &Output::Pattern);
        let bare = super::compile(&request(&[("off", "outro")], SEED), &Output::Pattern);
        assert_eq!((full.status, bare.status), (200, 200));
        let summary = |body: &str| body.lines().nth(1).unwrap().to_owned();
        assert_ne!(summary(&full.body), summary(&bare.body));
        assert!(
            summary(&full.body).starts_with("// 24 bars"),
            "{}",
            full.body
        );
        assert!(
            summary(&bare.body).starts_with("// 20 bars"),
            "{}",
            bare.body
        );
    }

    #[test]
    fn the_score_is_json_the_page_can_set_side_by_side() {
        let full = super::compile(&request(&[], SEED), &Output::Score);
        assert_eq!(full.status, 200, "{}", full.body);
        let body = full.body.trim_end();
        assert!(
            body.starts_with("{\"metrics\":[{\"name\":\"layering\""),
            "{body}"
        );
        assert!(body.ends_with('}'), "{body}");
        // The machine profile by default, so pulse and steadiness are there; and the edit test ran.
        assert!(body.contains("\"name\":\"steadiness\""), "{body}");
        assert!(!body.contains("\"legibility\":null"), "{body}");
        let interest = super::compile(&request(&[("profile", "interest")], SEED), &Output::Score);
        assert!(!interest.body.contains("steadiness"), "{}", interest.body);
        // The jazz profile: same metrics as machine, its own bands (recurrence 0.65..0.85).
        let jazz = super::compile(&request(&[("profile", "jazz")], SEED), &Output::Score);
        assert_eq!(jazz.status, 200, "{}", jazz.body);
        assert!(
            jazz.body.contains("\"name\":\"recurrence\",\"value\":"),
            "{}",
            jazz.body
        );
        assert!(
            jazz.body.contains("\"low\":0.650,\"high\":0.850"),
            "{}",
            jazz.body
        );
        // Leaving out the bass changes what is measured.
        let bare = super::compile(&request(&[("off", "bass")], SEED), &Output::Score);
        assert_ne!(full.body, bare.body);
        let unknown = super::compile(&request(&[("profile", "polka")], SEED), &Output::Score);
        assert_eq!(unknown.status, 400);
    }

    #[test]
    fn the_reading_speaks_every_line_once_in_order() {
        // The seed poem has eight lines; the reading speaks each once, in reading order, at either
        // pace, and the first is the first line, with the bytes it came from (I4).
        let response = super::compile(&request(&[], SEED), &Output::Reading);
        assert_eq!(response.status, 200, "{}", response.body);
        let body = &response.body;
        assert!(body.starts_with("{\"pattern\":\"// the reading"), "{body}");
        // The share of its bar a fitted line fills, reading.rs FILL (0.85), for the page to fit a
        // neural voice's rendered line to the same room.
        assert!(body.contains(",\"fill\":0.850,\"cues\":["), "{body}");
        assert!(
            body.contains(",\"natural\":\"// the reading at the natural pace"),
            "{body}"
        );
        let (fitted, rest) = body.split_once("\"lines\":[").unwrap();
        let (natural, unread) = rest.split_once("\"unread\":").unwrap();
        let lines = |part: &str| -> Vec<usize> {
            part.split("\"line\":")
                .skip(1)
                .filter_map(|rest| rest.split(',').next()?.parse().ok())
                .collect()
        };
        assert_eq!(
            lines(fitted),
            (0..8).collect::<Vec<_>>(),
            "in reading order"
        );
        assert_eq!(
            lines(natural),
            (0..8).collect::<Vec<_>>(),
            "in reading order"
        );
        // Spans are in the compiled source, after the 41-byte header the controls write
        // (`+++`, `[style]`, `tempo_range = [110, 130]`, `+++`, each with its newline). The line is
        // 28 bytes and seven syllables: Ty-ger Ty-ger burn-ing bright. In the text box it is the
        // first 28 units, and it ends on a comma, so the natural pace pauses 0.45 s after it. Its
        // words, by hand: "Tyger" 0..5 and 6..11, "burning" 13..20, "bright" 21..27.
        let first = "\"line\":0,\"stanza\":0,\"start\":41,\"end\":69,\"from\":0,\"to\":28,\"syllables\":7,\"pace\":";
        assert!(fitted.contains(first), "{fitted}");
        assert!(natural.contains(first), "{natural}");
        let words = "\"pause\":0.450,\"ends\":\"comma\",\"words\":[{\"from\":0,\"to\":5,\"syllables\":2},\
                     {\"from\":6,\"to\":11,\"syllables\":2},{\"from\":13,\"to\":20,\"syllables\":2},\
                     {\"from\":21,\"to\":27,\"syllables\":1}],\"text\":\"Tyger Tyger, burning bright,\"}";
        assert!(fitted.contains(words), "{fitted}");
        assert!(natural.contains(words), "{natural}");
        // Its pace is one of the seventeen hundredths 0.920..=1.080, the same at both paces.
        let pace = |part: &str| -> f64 {
            part.split_once(first)
                .and_then(|(_, rest)| rest.split(',').next()?.parse().ok())
                .unwrap()
        };
        assert!((0.92..=1.08).contains(&pace(fitted)), "{fitted}");
        assert_eq!(pace(fitted).to_bits(), pace(natural).to_bits());
        // The first stanza starts with the music, in bar 1, under the default `breath`.
        assert!(
            natural.starts_with("{\"onset\":\"breath\",\"slot\":0,\"line\":0,"),
            "{natural}"
        );
        assert!(
            natural.contains("{\"onset\":\"follows\",\"slot\":null,\"line\":1,"),
            "{natural}"
        );
        assert_eq!(unread, "[],\"natural_unread\":[]}\n", "nothing lost");
        // The music's clock, for the page to find a bar on: the bars start at cycle 0.
        assert!(body.contains(",\"bars\":[0,"), "{body}");
    }

    #[test]
    fn a_line_the_reading_loses_is_listed_for_the_page() {
        // Pied Beauty with the outro off — the page's setting B with the reading on breath. Eleven
        // lines in eleven bars; stanza 2 arrives in bar 7, waits for its breath to bar 8, and its
        // fifth line would fall in bar 12, past the piece (SPEC.md §4.7). The page is told.
        let poem = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/hopkins-pied-beauty.poem"),
        )
        .unwrap();
        let response = super::compile(
            &request(&[("set", "reading:breath,outro:off")], &poem),
            &Output::Reading,
        );
        assert_eq!(response.status, 200, "{}", response.body);
        let body = &response.body;
        let (cues, unread) = body.split_once("\"unread\":").unwrap();
        assert!(
            unread.starts_with("[{\"line\":10,")
                && unread.contains("\"why\":\"the piece ends before its turn\"")
                // The natural pace loses no line here by plan: whether the voice reaches the
                // last one is for the page to find out, and say.
                && unread.ends_with("Praise him.\"}],\"natural_unread\":[]}\n"),
            "{unread}"
        );
        // Not among the fitted cues; the natural pace's lines still list it.
        let (fitted, natural) = cues.split_once("\"lines\":[").unwrap();
        assert!(!fitted.contains("\"line\":10,"), "{fitted}");
        assert!(natural.contains("\"line\":10,"), "{natural}");
        assert!(cues.contains("// 1 of 11 lines not read:"), "{cues}");
        // The same piece under `arrives` loses nothing.
        let arrives = super::compile(
            &request(&[("set", "reading:arrives,outro:off")], &poem),
            &Output::Reading,
        );
        assert!(
            arrives
                .body
                .ends_with("\"unread\":[],\"natural_unread\":[]}\n"),
            "{}",
            arrives.body
        );
    }

    #[test]
    fn the_presets_are_answered_as_json_for_the_recording_tool() {
        let site = super::Site::new(super::Transport::Wasm).unwrap();
        for method in ["GET", "POST"] {
            let answer = site.respond(&super::Request::new(method, "/presets.json", String::new()));
            assert_eq!(answer.status, 200, "{method}");
            assert_eq!(answer.content_type, "application/json; charset=utf-8");
            assert!(
                answer
                    .body
                    .starts_with("[{\"name\":\"ball-karawane\",\"label\":")
            );
            assert!(answer.body.contains(
                "{\"name\":\"preseren-zdravljica\",\"label\":\"Prešeren — Zdravljica\",\"language\":\"sl\","
            ));
            assert_eq!(
                answer.body.matches("\"language\":").count(),
                super::PRESETS.len()
            );
        }
    }

    /// The settings the recording tool asks each reading under (`scripts/readings/lib.mjs`
    /// `SETTINGS`), as query pairs: every line any of them can speak is recorded.
    const RECORDED_SETTINGS: &[&[(&str, &str)]] = &[
        &[],
        &[("set", "reading:arrives")],
        &[("set", "reading:stanza")],
        &[("off", "outro")],
        &[("off", "outro"), ("set", "reading:arrives")],
    ];

    /// Every line a reading of `text` under `query` speaks, with its natural pace as the page is
    /// sent it.
    fn spoken_lines(text: &str, query: &[(&str, &str)]) -> Vec<(String, String)> {
        let request = request(query, text);
        let source = format!("{}{}", super::header(&request), super::body_of(text));
        let mappings = super::mappings(&request).unwrap();
        let poem = super::Poem::parse(source).unwrap();
        let style =
            super::Style::parse_optional(poem.frontmatter().and_then(|s| s.text(poem.source())))
                .unwrap();
        let phonemiser =
            super::Fallback::new(super::CmuDict::new(), super::LetterToSound::new()).unwrap();
        let lexicon = super::EnglishLexicon::new();
        let (score, _) = super::lower_poem(&poem, &phonemiser, &lexicon, &mappings).unwrap();
        let reading = super::reading::plan(&poem, &score, &style, &mappings).unwrap();
        reading
            .natural()
            .iter()
            .map(prosody_emit::reading::Natural::spoken)
            .chain(
                reading
                    .cues()
                    .iter()
                    .map(prosody_emit::reading::Cue::spoken),
            )
            .filter(|line| !line.text().trim().is_empty())
            .map(|line| {
                (
                    line.text().to_owned(),
                    prosody_emit::fmt_param(line.pace().speed()),
                )
            })
            .collect()
    }

    /// A string as JavaScript's `JSON.stringify` writes it, which is how the manifest has it.
    fn js_string(text: &str) -> String {
        let mut out = String::from("\"");
        for character in text.chars() {
            match character {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{8}' => out.push_str("\\b"),
                '\u{c}' => out.push_str("\\f"),
                other if u32::from(other) < 0x20 => {
                    let _ = std::fmt::Write::write_fmt(
                        &mut out,
                        format_args!("\\u{:04x}", u32::from(other)),
                    );
                }
                other => out.push(other),
            }
        }
        out.push('"');
        out
    }

    #[test]
    fn every_line_of_every_example_is_recorded_in_every_voice_of_its_language() {
        // `examples/readings/manifest.json`, made by `scripts/readings/render.mjs`: a line of the
        // file per voice and per recorded line. This checks it against the compiler as it is now,
        // so a change that alters a line's text or its pace — and so leaves the recordings stale —
        // fails here until they are rendered again. (That each key is its text's hash, the tool's
        // own `--check` verifies.)
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/readings");
        let manifest = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
        let voices: Vec<(String, String)> = manifest
            .lines()
            .filter(|line| line.contains("{\"engine\":"))
            .map(|line| {
                let id = line.trim().split('"').nth(1).unwrap().to_owned();
                let lang = line.split("\"lang\":\"").nth(1).unwrap();
                (id, lang.split('"').next().unwrap().to_owned())
            })
            .collect();
        let speaking = |language: &str| -> Vec<&str> {
            voices
                .iter()
                .filter(|(_, lang)| lang == language)
                .map(|(id, _)| id.as_str())
                .collect()
        };
        assert!(speaking("en").len() >= 3, "{voices:?}");
        assert_eq!(speaking("sl"), ["piper:sl_SI-artur-medium"]);

        let mut crane = 0;
        for (name, _, language, text) in super::PRESETS {
            let mut lines = std::collections::BTreeMap::new();
            for query in RECORDED_SETTINGS {
                lines.extend(spoken_lines(text, query));
            }
            assert!(!lines.is_empty(), "{name}");
            if *name == "crane-in-the-desert" {
                crane = lines.len();
            }
            for (line, pace) in &lines {
                let entry = format!(
                    "{{\"text\":{},\"pace\":{pace},\"recordings\":{{",
                    js_string(line)
                );
                let found = manifest.lines().find(|row| row.contains(&entry));
                assert!(
                    found.is_some(),
                    "{name}: no recording of {line:?} at pace {pace}"
                );
                let found = found.unwrap();
                for voice in speaking(language) {
                    let after = found.split(&format!("\"{voice}\":{{\"file\":\"")).nth(1);
                    assert!(after.is_some(), "{name}: {line:?} has no {voice}");
                    let file = after.unwrap().split('"').next().unwrap();
                    // The file is named for the pace it was spoken at, so audio rendered at an
                    // older pace can never be listed as this one (scripts/readings `fileOf`).
                    assert!(
                        file.ends_with(&format!("-{pace}.mp3")),
                        "{name}: {file} is not at pace {pace}"
                    );
                    let bytes = std::fs::metadata(dir.join(file)).map_or(0, |m| m.len());
                    assert!(bytes > 0, "{name}: {file} is missing or empty");
                }
            }
        }
        // Counted by hand from examples/crane-in-the-desert.poem: ten lines in two stanzas, each
        // spoken by some setting, none repeated — so ten recordings in each English voice.
        assert_eq!(crane, 10);
    }

    #[test]
    fn every_example_is_a_preset() {
        let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let mut files: Vec<String> = std::fs::read_dir(examples)
            .unwrap()
            .filter_map(|entry| {
                let name = entry.unwrap().file_name().into_string().unwrap();
                name.strip_suffix(".poem").map(str::to_owned)
            })
            .collect();
        files.sort();
        let presets: Vec<&str> = super::PRESETS.iter().map(|(name, _, _, _)| *name).collect();
        assert_eq!(files, presets);
    }

    #[test]
    fn a_preset_brings_its_palette_to_the_controls() {
        let json = super::presets_json().unwrap();
        // The Tyger's header, as the controls spell it — and its poem without the header. Its
        // `synth = "karplus"` sets both pitched instruments; the mix is the default preset's.
        let tyger = json
            .split("{\"name\":")
            .find(|preset| preset.starts_with("\"blake-tyger\""))
            .unwrap();
        for field in [
            "\"scale\":\"phrygian_dominant\"",
            "\"root\":\"D\"",
            "\"bank\":\"tr909\"",
            "\"bass\":\"karplus\"",
            "\"melody\":\"karplus\"",
            "\"low\":\"124\"",
            "\"high\":\"132\"",
            "\"swing\":\"0.08\"",
            "\"reverb\":\"0.25\"",
            "\"humanise\":\"0.20\"",
        ] {
            assert!(tyger.contains(field), "{field} in {tyger}");
        }
        assert!(
            json.contains("\"poem\":\"Tyger Tyger, burning bright,"),
            "{json}"
        );
        // A poem with no header takes the defaults.
        assert!(
            json.contains("\"root\":\"C\",\"scale\":\"minor\""),
            "{json}"
        );
        assert_eq!(super::root_label("cs"), "C#");
        assert_eq!(super::root_label("a"), "A");
    }

    #[test]
    fn the_controls_write_a_header_the_style_reads_back() {
        // Every control's value, in header units, must arrive in the palette — a key the header
        // dropped would leave the page showing one palette and playing another.
        for name in ["warm", "jazz"] {
            let palette = prosody_emit::Style::preset(name).unwrap();
            let pairs = controls(&palette);
            let query: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
            assert_eq!(
                prosody_emit::Style::parse(&super::header(&request(&query, ""))).unwrap(),
                palette,
                "{name}"
            );
        }
        let warm = prosody_emit::Style::preset("warm").unwrap();
        let fields = warm.describe();
        let mut query: Vec<(String, String)> = fields
            .iter()
            .filter(|(key, _)| !matches!(**key, "tempo_range" | "root"))
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect();
        query.push(("root".to_owned(), "C".to_owned()));
        let pairs: Vec<(&str, &str)> = query
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let header = super::header(&request(&pairs, ""));
        assert_eq!(
            prosody_emit::Style::parse(&header).unwrap(),
            warm,
            "{header}"
        );

        // A control the request leaves out takes the default; a number that is not one falls
        // back rather than breaking the header around it.
        let partial = super::header(&request(&[("reverb", "lots"), ("delay", "0.5")], ""));
        let style = prosody_emit::Style::parse(&partial).unwrap();
        assert_eq!(style.mix().delay().hundredths(), 50);
        assert_eq!(
            style.mix().reverb(),
            prosody_emit::Style::default().mix().reverb()
        );
        // A name is quoted, so it cannot write a second key.
        let sneaky = super::header(&request(&[("bank", "tr909\"\nscale = \"lydian")], ""));
        assert!(prosody_emit::Style::parse(&sneaky).is_err(), "{sneaky}");
    }

    /// Every control's value for a palette, as the page sends it.
    fn controls(style: &prosody_emit::Style) -> Vec<(&'static str, String)> {
        let mut query: Vec<(&'static str, String)> = style
            .describe()
            .into_iter()
            .filter(|(key, _)| !matches!(*key, "tempo_range" | "root"))
            .collect();
        query.push(("root", super::root_label(&style.describe()["root"])));
        query
    }

    #[test]
    fn the_jazz_palette_plays_rhodes_sevenths_over_the_ride() {
        let jazz = prosody_emit::Style::preset("jazz").unwrap();
        let pairs: Vec<(&str, String)> = controls(&jazz);
        let query: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let response = super::compile(&request(&query, SEED), &Output::Pattern);
        assert_eq!(response.status, 200, "{}", response.body);
        let body = &response.body;
        assert!(body.contains(".bank(\"RolandR8\")"), "{body}");
        // A drum lane of ride strokes. Not "the first bar is a ride bar": 24 bars are more than
        // a level holds, so the first element may be a group of bars (`strudel::nested`).
        assert!(
            body.lines()
                .any(|l| l.trim_start().starts_with("s(\"<") && l.contains("[rd rd]")),
            "the ride keeps time: {body}"
        );
        assert!(body.contains(".swingBy(0.330,"), "triplet swing: {body}");
        // C dorian, degree 0 as a seventh: C E♭ G B♭ from 60 — 60 63 67 70.
        assert!(body.contains("[60,63,67,70]"), "{body}");
        assert!(body.contains("//   voicing = seventh"), "{body}");
    }

    #[test]
    fn under_the_harmony_the_jazz_melody_is_single_notes_over_the_chords() {
        // The same jazz controls as above, with B's harmony on: degree 0 is the single note 60,
        // not the chord [60,63,67,70], and the chord lane keeps the sevenths (an octave down,
        // from 48: C dorian degree 0 is 48 51 55 58).
        let jazz = prosody_emit::Style::preset("jazz").unwrap();
        let pairs: Vec<(&str, String)> = controls(&jazz);
        let mut query: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        query.push(("set", "harmony:cadence"));
        let response = super::compile(&request(&query, SEED), &Output::Pattern);
        assert_eq!(response.status, 200, "{}", response.body);
        let body = &response.body;
        let melody = body.lines().find(|l| l.contains("// the melody")).unwrap();
        let notes = melody.split('"').nth(1).unwrap();
        assert!(!notes.contains(','), "{melody}");
        assert!(notes.contains(" 60") || notes.contains("[60"), "{melody}");
        let chords = body.lines().find(|l| l.contains("// the chords")).unwrap();
        assert!(
            chords.split('"').nth(1).unwrap().contains(",58]"),
            "{chords}"
        );
        assert!(body.contains("as single notes over the chords"), "{body}");
        let explain = super::compile(&request(&query, SEED), &Output::Explain);
        assert!(
            explain
                .body
                .contains("\nrhyme notes   single notes over the chords"),
            "{}",
            explain.body
        );
    }

    #[test]
    fn the_register_the_ride_stand_in_and_density_reach_the_pattern() {
        let query = [
            ("bass_octave", "1"),
            ("melody_octave", "5"),
            ("ride_fallback", "crash"),
            ("density_level", "0.5"),
            ("kit", "ride"),
            ("bank", "tr808"),
        ];
        let response = super::compile(&request(&query, SEED), &Output::Pattern);
        assert_eq!(response.status, 200, "{}", response.body);
        let body = &response.body;
        for line in [
            "//   bass_octave = 1\n",
            "//   melody_octave = 5\n",
            "//   ride_fallback = crash\n",
            "//   density_level = 0.50\n",
            "crash in place of the ride, which RolandTR808 does not have,",
            "//   density: every gain in a bar divided by n^0.500",
        ] {
            assert!(body.contains(line), "{line} missing: {body}");
        }
        // An octave outside its role's range is refused at the boundary, naming the key.
        let response = super::compile(&request(&[("bass_octave", "4")], SEED), &Output::Pattern);
        assert_eq!(response.status, 400);
        assert!(response.body.contains("bass_octave"), "{}", response.body);
    }

    #[test]
    fn every_preset_compiles() {
        for (name, _, _, text) in super::PRESETS {
            let response = super::compile(&request(&[], body_of(text)), &Output::Pattern);
            assert_eq!(response.status, 200, "{name}: {}", response.body);
        }
    }

    #[test]
    fn the_trace_points_at_the_text_box_not_the_source() {
        // Hand-worked: "Tyger" is the first word of the seed, so some mark starts at 0 of the text
        // box. With a pasted header of its own in front, the same syllable is that header's
        // length further on — counted in UTF-16 units, where "—" is one unit and three bytes.
        let plain = super::compile(&request(&[], SEED), &Output::Trace);
        assert_eq!(plain.status, 200, "{}", plain.body);
        assert!(
            plain.body.contains("{\"start\":0,\"end\":"),
            "{}",
            plain.body
        );
        assert!(plain.body.contains("\"pattern\":\"// the trace:"));

        let pasted = format!("+++\n# a note — mine\n+++\n{SEED}");
        let shift = "+++\n# a note — mine\n+++\n".encode_utf16().count();
        let shifted = super::compile(&request(&[], &pasted), &Output::Trace);
        assert_eq!(shifted.status, 200, "{}", shifted.body);
        assert!(
            shifted
                .body
                .contains(&format!("{{\"start\":{shift},\"end\":")),
            "{}",
            shifted.body
        );
        // Every mark lands on a letter of the text box, never on the header the page adds.
        let typed: Vec<u16> = pasted.encode_utf16().collect();
        for piece in shifted.body.split("\"start\":").skip(1) {
            let start: usize = piece.split(',').next().unwrap().parse().unwrap();
            assert!(start >= shift && start < typed.len(), "{start}");
        }
    }

    #[test]
    fn a_reading_cue_says_where_its_line_is_in_the_text_box() {
        // Hand-worked: the seed's first line, "Tyger Tyger, burning bright," is 28 units long and
        // is read first; with a pasted header in front, it moves by that header's length.
        let plain = super::compile(&request(&[], SEED), &Output::Reading);
        assert!(
            plain.body.contains("\"from\":0,\"to\":28,"),
            "{}",
            plain.body
        );
        let pasted = format!("+++\n# a note — mine\n+++\n{SEED}");
        let shift = "+++\n# a note — mine\n+++\n".encode_utf16().count();
        let shifted = super::compile(&request(&[], &pasted), &Output::Reading);
        assert!(
            shifted
                .body
                .contains(&format!("\"from\":{shift},\"to\":{},", shift + 28)),
            "{}",
            shifted.body
        );
    }

    #[test]
    fn the_seed_poem_compiles() {
        // The page opens by compiling it, so a broken one would greet every visitor with an error.
        let rendered = super::render(
            SEED,
            SEED,
            &super::Output::Pattern,
            &Mappings::all(),
            Profile::Machine,
        )
        .expect("the seed poem");
        assert!(rendered.contains("stack("), "{rendered}");
    }

    #[test]
    fn the_trace_says_where_the_bars_fall_under_either_pulse() {
        // Wilde's Theocritus: 126 beats over 33 slots, bars of three and four (PLAN.md, the
        // off-rhythm trace). Under the steady pulse a cycle is a beat, so the trace's bar starts
        // run from 0 to 126 in steps of 3 and 4, at the tempo itself; under the elastic pulse a
        // cycle is a bar, so they count 0 to 33 at 126 / 33's integer mean, 3, below it.
        let wilde = super::PRESETS
            .iter()
            .find(|(name, ..)| *name == "wilde-theocritus")
            .map(|(_, _, _, text)| body_of(text))
            .unwrap();
        let bars_and_cpm = |query: &[(&str, &str)]| -> (Vec<u64>, f64) {
            let response = super::compile(&request(query, wilde), &Output::Trace);
            assert_eq!(response.status, 200, "{}", response.body);
            let bars = response.body.split("\"bars\":[").nth(1).unwrap();
            let bars: Vec<u64> = bars[..bars.find(']').unwrap()]
                .split(',')
                .map(|n| n.parse().unwrap())
                .collect();
            let cpm = response.body.split("\"cpm\":").nth(1).unwrap();
            let cpm: f64 = cpm[..cpm.find(',').unwrap()].parse().unwrap();
            (bars, cpm)
        };
        let (steady, steady_cpm) = bars_and_cpm(&[]);
        assert_eq!(steady.len(), 34);
        assert_eq!(steady.last(), Some(&126));
        let lengths: std::collections::BTreeSet<u64> =
            steady.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert_eq!(lengths, [3, 4].into_iter().collect());
        let (elastic, elastic_cpm) = bars_and_cpm(&[("set", "pulse:elastic")]);
        assert_eq!(elastic, (0..=33).collect::<Vec<u64>>());
        // The elastic cycle is three steady beats: 3 × 40.333 is the tempo, 121, to rounding.
        assert!((steady_cpm - 121.0).abs() < 1e-9, "{steady_cpm}");
        assert!(
            (elastic_cpm * 3.0 - steady_cpm).abs() < 0.01,
            "{elastic_cpm}"
        );
        // `off=pulse` is the pulse's first variant, elastic: the pulse has no off.
        assert_eq!(bars_and_cpm(&[("off", "pulse")]).0, elastic);
        // ...and the pattern and the explain view say which it is.
        let pattern = super::compile(
            &request(&[("set", "pulse:elastic")], wilde),
            &Output::Pattern,
        );
        assert!(
            pattern.body.contains("//   pulse = elastic"),
            "{}",
            pattern.body
        );
        let explain = super::compile(
            &request(&[("set", "pulse:elastic")], wilde),
            &Output::Explain,
        );
        assert!(explain.body.contains("pulse=elastic"), "{}", explain.body);
        assert!(explain.body.contains("pulse elastic ·"), "{}", explain.body);
    }

    #[test]
    fn light_function_words_reach_the_pattern_the_explain_view_and_the_score() {
        // The Tyger, asked for with `set=function-words:light`: the pattern header and the explain
        // view name it and agree on how many syllables it struck lightly; "or" in "hand or eye"
        // is one of them; the score's syncopation is measured on the lightened accents. Under
        // the default none of it appears.
        let tyger = super::PRESETS
            .iter()
            .find(|(name, ..)| *name == "blake-tyger")
            .map(|(_, _, _, text)| body_of(text))
            .unwrap();
        let light = &[("set", "function-words:light")];
        let pattern = super::compile(&request(light, tyger), &Output::Pattern);
        let explain = super::compile(&request(light, tyger), &Output::Explain);
        assert_eq!(pattern.status, 200, "{}", pattern.body);
        assert_eq!(explain.status, 200, "{}", explain.body);
        let marked = explain
            .body
            .lines()
            .filter(|line| line.ends_with(",function-words"))
            .count();
        assert!(marked > 0, "{}", explain.body);
        assert!(
            pattern.body.contains(&format!(
                "//   function-words = light: a one-syllable function word off the metre's \
                 strong positions is struck at 2/3 of full force, in {marked} syllable(s)"
            )),
            "{}",
            pattern.body
        );
        assert!(
            explain.body.contains("function-words=light"),
            "{}",
            explain.body
        );
        let or = explain
            .body
            .lines()
            .find(|line| line.trim_start().starts_with("or             2.2.1/2"))
            .unwrap();
        assert!(or.contains(" 0.67 "), "{or}");
        let full = super::compile(&request(&[], tyger), &Output::Pattern);
        assert!(!full.body.contains("function-words"));
        let sync = |query: &[(&str, &str)]| -> String {
            let score = super::compile(&request(query, tyger), &Output::Score);
            assert_eq!(score.status, 200, "{}", score.body);
            score
                .body
                .lines()
                .find(|line| line.contains("syncopation"))
                .unwrap()
                .to_owned()
        };
        assert_ne!(sync(&[]), sync(light));
        assert!(
            super::compile(
                &request(&[("set", "function-words:off")], tyger),
                &Output::Pattern
            )
            .status
                >= 400
        );
    }

    #[test]
    fn long_stanzas_erode_in_the_pattern_the_explain_view_and_the_score() {
        // Lowell's "Patterns", the preset, under the default: the pattern header and the explain
        // view count the same 240 bars over an eroding loop, and the score measures the eroded
        // piece. `set=long-stanzas:gradual` is the old handover — named in the explain view's
        // list of choices — and a short poem is the same under both. `off` is refused.
        let patterns = super::PRESETS
            .iter()
            .find(|(name, ..)| *name == "lowell-patterns")
            .map(|(_, _, _, text)| body_of(text))
            .unwrap();
        let pattern = super::compile(&request(&[], patterns), &Output::Pattern);
        let explain = super::compile(&request(&[], patterns), &Output::Explain);
        let score = super::compile(&request(&[], patterns), &Output::Score);
        assert_eq!(pattern.status, 200, "{}", pattern.body);
        assert_eq!(explain.status, 200, "{}", explain.body);
        assert_eq!(score.status, 200, "{}", score.body);
        assert!(
            pattern
                .body
                .contains("//   long-stanzas = erode: 240 bar(s) over an eroding loop"),
            "{}",
            &pattern.body[..1500]
        );
        assert!(
            explain
                .body
                .contains("240 bar(s) over an eroding loop (§4.6)"),
            "{}",
            explain.body
        );
        assert!(explain.body.contains("  erodes pass 1 keeps"));
        assert!(score.body.contains("overall"), "{}", score.body);
        let gradual = &[("set", "long-stanzas:gradual")];
        let seed = super::compile(&request(gradual, SEED), &Output::Pattern);
        assert_eq!(
            seed.body,
            super::compile(&request(&[], SEED), &Output::Pattern).body
        );
        let named = super::compile(&request(gradual, SEED), &Output::Explain);
        assert!(
            named.body.contains("long-stanzas=gradual"),
            "{}",
            named.body
        );
        assert!(
            super::compile(
                &request(&[("set", "long-stanzas:off")], patterns),
                &Output::Pattern
            )
            .status
                >= 400
        );
    }

    #[test]
    fn articulated_drums_reach_the_pattern_and_the_explain_view() {
        let tyger = body_of(SEED);
        let articulated = &[("set", "drums:articulated")];
        let pattern = super::compile(&request(articulated, tyger), &Output::Pattern);
        let explain = super::compile(&request(articulated, tyger), &Output::Explain);
        assert_eq!(pattern.status, 200, "{}", pattern.body);
        assert_eq!(explain.status, 200, "{}", explain.body);
        assert!(
            pattern.body.contains("//   drums = articulated:"),
            "{}",
            pattern.body
        );
        assert!(
            pattern.body.contains("[[bd hh] [sd hh] [bd hh] sd]@4"),
            "{}",
            pattern.body
        );
        assert!(
            explain.body.contains("drums=articulated")
                && explain.body.contains("drums articulated ·")
                && explain.body.contains(",drums(pulse)")
                && explain.body.contains(",drums(backbeat)")
                && explain.body.contains(",drums(subdivision)"),
            "{}",
            explain.body
        );
        assert!(
            super::compile(&request(&[("set", "drums:off")], tyger), &Output::Pattern).status
                >= 400
        );
    }

    #[test]
    fn slant_rhyme_reaches_the_pattern_and_the_explain_view() {
        let theocritus = super::PRESETS
            .iter()
            .find(|(name, ..)| *name == "wilde-theocritus")
            .map(|(_, _, _, text)| body_of(text))
            .unwrap();
        let slant = &[("set", "rhyme:slant")];
        let pattern = super::compile(&request(slant, theocritus), &Output::Pattern);
        let explain = super::compile(&request(slant, theocritus), &Output::Explain);
        assert_eq!(pattern.status, 200, "{}", pattern.body);
        assert_eq!(explain.status, 200, "{}", explain.body);
        assert!(
            pattern.body.contains("//   rhyme = slant:"),
            "{}",
            pattern.body
        );
        assert!(
            explain.body.contains("rhyme=slant") && explain.body.contains("rhyme slant ·"),
            "{}",
            explain.body
        );
        assert!(
            super::compile(
                &request(&[("set", "rhyme:off")], theocritus),
                &Output::Pattern
            )
            .status
                >= 400
        );
    }

    #[test]
    fn anchor_reaches_the_pattern_and_the_explain_view() {
        let naidu = include_str!("../../../examples/naidu-bazaars-of-hyderabad.poem");
        let default_pattern = super::compile(&request(&[], naidu), &Output::Pattern);
        assert_eq!(default_pattern.status, 200);
        assert!(!default_pattern.body.contains("anchor = on"));

        let anchor_on = &[("set", "anchor:on")];
        let pattern = super::compile(&request(anchor_on, naidu), &Output::Pattern);
        let explain = super::compile(&request(anchor_on, naidu), &Output::Explain);
        assert_eq!(pattern.status, 200);
        assert_eq!(explain.status, 200);
        assert!(
            pattern
                .body
                .contains("//   anchor = on: anaphora ostinatos accented"),
            "{}",
            pattern.body
        );
        assert!(
            explain.body.contains("anchor=on") && explain.body.contains("anchor on ·"),
            "{}",
            explain.body
        );
    }

    #[test]
    fn melody_reaches_the_pattern_and_the_explain_view() {
        let crane = include_str!("../../../examples/crane-in-the-desert.poem");
        let default_pattern = super::compile(&request(&[], crane), &Output::Pattern);
        assert_eq!(default_pattern.status, 200);
        assert!(!default_pattern.body.contains("melody = assonance"));

        let melody_assonance = &[("set", "melody:assonance")];
        let pattern = super::compile(&request(melody_assonance, crane), &Output::Pattern);
        let explain = super::compile(&request(melody_assonance, crane), &Output::Explain);
        assert_eq!(pattern.status, 200);
        assert_eq!(explain.status, 200);
        assert!(
            pattern
                .body
                .contains("//   melody = assonance: line-internal assonance figures give melody"),
            "{}",
            pattern.body
        );
        assert!(
            explain.body.contains("melody=assonance")
                && explain.body.contains("melody assonance ·"),
            "{}",
            explain.body
        );
    }

    #[test]
    fn the_straight_stack_reaches_the_pattern_the_trace_and_the_score() {
        // Wilde's Theocritus, whose bars are of three and four beats, so a `@2` inside a bar is
        // only ever a fitted long syllable. Asked for with `set=stack:straight`, the music's drum
        // sequences and the trace's lanes carry the same fitted beats — the trace is the music's
        // walk — and the jazz score counts no crossed beat.
        let wilde = super::PRESETS
            .iter()
            .find(|(name, ..)| *name == "wilde-theocritus")
            .map(|(_, _, _, text)| body_of(text))
            .unwrap();
        // How many syllables hold two parts, in every `call("<…>")` sequence of `text`.
        let held = |text: &str, call: &str| -> usize {
            // In the trace's JSON a quote is escaped and `<` written `\u003c`.
            let needle = format!("{call}(\\\"\\u003c");
            let plain = format!("{call}(\"<");
            text.split(&needle)
                .skip(1)
                .chain(text.split(&plain).skip(1))
                .map(|rest| {
                    let end = rest.find('>').unwrap_or(rest.len());
                    rest[..end].matches("@2 ").count() + rest[..end].matches("@2]").count()
                })
                .sum()
        };
        let straight = &[("set", "stack:straight")];
        let pattern = super::compile(&request(straight, wilde), &Output::Pattern);
        let trace = super::compile(&request(straight, wilde), &Output::Trace);
        assert_eq!(pattern.status, 200, "{}", pattern.body);
        assert_eq!(trace.status, 200, "{}", trace.body);
        assert!(
            pattern.body.contains("//   stack = straight"),
            "{}",
            pattern.body
        );
        let fitted = held(&pattern.body, "s");
        assert!(fitted >= 38, "{fitted}");
        assert_eq!(held(&trace.body, "n"), fitted);
        // Under the default, nothing is held two parts and nothing is named.
        let crossing = super::compile(&request(&[], wilde), &Output::Pattern);
        assert_eq!(held(&crossing.body, "s"), 0);
        assert!(!crossing.body.contains("stack ="));
        let crossing_trace = super::compile(&request(&[], wilde), &Output::Trace);
        assert_eq!(held(&crossing_trace.body, "n"), 0);
        // The score reads the same onsets.
        let score = super::compile(
            &request(&[("set", "stack:straight"), ("profile", "jazz")], wilde),
            &Output::Score,
        );
        assert!(score.body.contains("cross_rhythm"), "{}", score.body);
        let row = score
            .body
            .lines()
            .find(|line| line.contains("cross_rhythm"))
            .unwrap();
        assert!(row.contains("0.000"), "{row}");
        let explain = super::compile(&request(straight, wilde), &Output::Explain);
        assert!(
            explain.body.contains("stack straight ·"),
            "{}",
            explain.body
        );
        assert!(
            explain.body.contains("  straight 3 into 2 at"),
            "{}",
            explain.body
        );
    }
}
