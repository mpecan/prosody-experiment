//! `prosody-web` — write a poem in a browser and hear it.
//!
//! `SPEC.md` §7.6 wants the cheapest possible audition path, and §8 wants the debug view alongside
//! it. A page with the poem on one side and both the pattern and the annotation on the other is
//! the cheapest that exists: type a word, hear the difference, and see which pass caused it.
//!
//! Like `prosody-cli`, this crate is all IO. Every musical decision belongs to `prosody-core` and
//! every palette decision to `prosody-emit`; nothing here does more than move text.
//!
//! The page also compares. Each candidate mapping ([`Mapping`]) can be left out of either of two
//! settings, A and B; the page plays one, shows both patterns, and measures both with
//! `prosody-score`, so what a mapping adds can be heard and read side by side.

use std::fmt::Write as _;

use anyhow::{Context as _, Result};
use prosody_cmudict::CmuDict;
use prosody_core::annotate;
use prosody_core::lower::{self, Analyses};
use prosody_core::mapping::{Mapping, Mappings};
use prosody_core::voice::Score;
use prosody_core::{Fallback, Poem, meter, repetition, rhyme, role, sound, surprisal};
use prosody_emit::style::{BANKS, PRESETS as PALETTES, SOUNDFONTS, SYNTHS};
use prosody_emit::{Backend as _, Scale, Strudel, Style};
use prosody_lexicon::EnglishLexicon;
use prosody_lts::LetterToSound;
use prosody_score::{DEFAULT_PROBES, Profile, Report, sensitivity};

mod server;

use server::{Request, Response};

/// The page, with the option lists and a starting poem substituted in.
///
/// Substituted rather than hard-coded in the HTML so that the controls cannot drift from what
/// [`Style`] actually accepts — adding a scale in one place adds it to the page.
fn page() -> Result<String> {
    let list = |values: &[&str]| {
        let quoted: Vec<String> = values.iter().map(|value| format!("\"{value}\"")).collect();
        format!("[{}]", quoted.join(","))
    };
    let scales: Vec<&str> = Scale::ALL.iter().map(|scale| scale.name()).collect();
    let mappings: Vec<String> = Mapping::ALL
        .iter()
        .map(|mapping| {
            format!(
                "[{},{}]",
                json_string(mapping.name()),
                json_string(mapping.describe())
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

    Ok(include_str!("index.html")
        .replace("__PRESETS__", &presets_json()?)
        .replace("__PALETTES__", &format!("{{{}}}", palettes.join(",")))
        .replace("__SCALES__", &list(&scales))
        .replace("__BANKS__", &list(BANKS))
        .replace("__INSTRUMENTS__", &list(&instruments))
        .replace("__MAPPINGS__", &format!("[{}]", mappings.join(",")))
        .replace("__SEED_POEM__", &json_string(SEED)))
}

/// The example poems, for the preset menu: a file name and a label, then the file itself.
///
/// Embedded rather than read at run time so the binary works from any directory; a test checks
/// that every file in `examples/` is here, so a new example cannot be missing from the menu.
const PRESETS: &[(&str, &str, &str)] = &[
    (
        "blake-tyger",
        "Blake — The Tyger",
        include_str!("../../../examples/blake-tyger.poem"),
    ),
    (
        "carroll-jabberwocky",
        "Carroll — Jabberwocky",
        include_str!("../../../examples/carroll-jabberwocky.poem"),
    ),
    (
        "dickinson-because",
        "Dickinson — Because I could not stop for Death",
        include_str!("../../../examples/dickinson-because.poem"),
    ),
    (
        "hopkins-pied-beauty",
        "Hopkins — Pied Beauty",
        include_str!("../../../examples/hopkins-pied-beauty.poem"),
    ),
    (
        "poe-annabel-lee",
        "Poe — Annabel Lee",
        include_str!("../../../examples/poe-annabel-lee.poem"),
    ),
    (
        "preseren-zdravljica",
        "Prešeren — Zdravljica",
        include_str!("../../../examples/preseren-zdravljica.poem"),
    ),
    (
        "shakespeare-sonnet-18",
        "Shakespeare — Sonnet 18",
        include_str!("../../../examples/shakespeare-sonnet-18.poem"),
    ),
    (
        "whitman-song-of-myself",
        "Whitman — Song of Myself",
        include_str!("../../../examples/whitman-song-of-myself.poem"),
    ),
    (
        "wilde-theocritus",
        "Wilde — Theocritus",
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
    for (name, label, text) in PRESETS {
        let poem = Poem::parse((*text).to_owned()).with_context(|| format!("preset {name}"))?;
        let style = Style::parse_optional(poem.frontmatter().and_then(|s| s.text(poem.source())))
            .with_context(|| format!("preset {name}'s palette"))?;
        presets.push(format!(
            "{{\"name\":{},\"label\":{},\"poem\":{},\"palette\":{}}}",
            json_string(name),
            json_string(label),
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

/// A JSON string literal, for embedding a poem in the page.
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
            // `<` would otherwise be able to close the surrounding `<script>` element.
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

fn main() -> Result<()> {
    let port = std::env::args()
        .nth(1)
        .map_or(Ok(4747), |given| given.parse())
        .context("the port must be a number")?;

    let rendered = page()?;
    server::serve(port, move |request| {
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/") => Response::html(rendered.clone()),
            ("GET", "/strudel-globals.mjs") => {
                Response::javascript(include_str!("strudel-globals.mjs").to_owned())
            }
            ("POST", "/pattern") => compile(request, &Output::Pattern),
            ("POST", "/explain") => compile(request, &Output::Explain),
            ("POST", "/score") => compile(request, &Output::Score),
            _ => Response::problem(404, "no such thing here\n".to_owned()),
        }
    })
    .context("serving")?;
    Ok(())
}

/// Which of the views the page asked for.
enum Output {
    Pattern,
    Explain,
    /// The measurements, as JSON, so the page can set two settings side by side.
    Score,
}

/// Compiles the request body under the palette in its query string.
///
/// The poem is the body and the palette is the query, so nothing has to encode a poem — no JSON,
/// no escaping, and a body that is exactly the bytes the writer typed.
fn compile(request: &Request, output: &Output) -> Response {
    let source = format!("{}{}", header(request), body_of(&request.body));
    let Some(mappings) = mappings(request) else {
        return Response::problem(400, "unknown mapping in `off`\n".to_owned());
    };
    let profile = match request.param("profile") {
        None => Profile::Machine,
        Some(name) => match Profile::named(name) {
            Some(profile) => profile,
            None => return Response::problem(400, format!("unknown profile `{name}`\n")),
        },
    };
    match render(&source, output, &mappings, profile) {
        Ok(text) => Response::text(text),
        // A poem that will not compile is what the middle of a keystroke looks like, so the reason
        // goes back with a 400 and the page shows it rather than blanking.
        Err(error) => Response::problem(400, format!("{error:#}\n")),
    }
}

/// The palette keys the page sends that are names, written as TOML strings.
const NAMED_KEYS: &[&str] = &["scale", "root", "bank", "bass", "melody"];

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

/// The mappings the request leaves on: all of them, less those named in `off` (comma-separated).
/// `None` if it names one that does not exist, so a typo is an error rather than a silent no-op.
fn mappings(request: &Request) -> Option<Mappings> {
    request
        .param("off")
        .unwrap_or("")
        .split(',')
        .filter(|name| !name.is_empty())
        .try_fold(Mappings::all(), |set, name| {
            Mapping::named(name).map(|mapping| set.without(mapping))
        })
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

/// Compiles a poem, the way the CLI does.
fn render(source: &str, output: &Output, mappings: &Mappings, profile: Profile) -> Result<String> {
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
            for (key, value) in style.describe() {
                let _ = writeln!(out, "{key:<12} {value}");
            }
            let off: Vec<&str> = mappings.off().map(Mapping::name).collect();
            if !off.is_empty() {
                let _ = writeln!(out, "{:<12} {}", "left out", off.join(", "));
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
    }
}

/// Every analysis pass and the lowering, under `mappings`.
fn lower_poem(
    poem: &Poem,
    phonemiser: &impl prosody_core::Phonemiser,
    lexicon: &impl prosody_core::Lexicon,
    mappings: &Mappings,
) -> Result<(Score, prosody_core::PoemRoles)> {
    let meter = meter::scan(poem, phonemiser);
    let rhyme = rhyme::analyse(poem, phonemiser);
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
    use super::{Mapping, Mappings, Output, Profile, SEED, body_of, json_string, page};

    #[test]
    fn the_page_lists_every_palette_value_the_style_accepts() {
        // Substituted from `Style`'s own tables rather than typed into the HTML, so a scale added
        // in one place cannot be missing from the other.
        let html = page().unwrap();
        for scale in prosody_emit::Scale::ALL {
            assert!(html.contains(scale.name()), "{} missing", scale.name());
        }
        for name in prosody_emit::style::BANKS
            .iter()
            .chain(prosody_emit::style::SYNTHS)
            .chain(prosody_emit::style::SOUNDFONTS)
        {
            assert!(html.contains(&format!("\"{name}\"")), "{name} missing");
        }
        // Every named palette, with its values, for the sound menu.
        for name in prosody_emit::style::PRESETS {
            assert!(
                html.contains(&format!("\"{name}\":{{")),
                "palette {name} missing"
            );
        }
        for placeholder in ["__PALETTES__", "__INSTRUMENTS__", "__PRESETS__"] {
            assert!(!html.contains(placeholder), "{placeholder} left unfilled");
        }
        assert!(
            !html.contains("__SCALES__"),
            "a placeholder was left unfilled"
        );
        assert!(!html.contains("__SEED_POEM__"));
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
    fn a_poem_cannot_break_out_of_the_script_element() {
        // The seed poem is embedded in a `<script>` block, so `</script>` inside it would end the
        // block early and the rest would render as markup.
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
    fn the_page_lists_every_mapping_the_core_can_leave_out() {
        let html = page().unwrap();
        for mapping in Mapping::ALL {
            assert!(html.contains(mapping.describe()), "{mapping:?} missing");
        }
        assert!(!html.contains("__MAPPINGS__"));
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
        assert!(super::mappings(&request(&[("off", "bass,polka")], "")).is_none());
        let refused = super::compile(&request(&[("off", "polka")], SEED), &Output::Pattern);
        assert_eq!(refused.status, 400);
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
        // Leaving out the bass changes what is measured.
        let bare = super::compile(&request(&[("off", "bass")], SEED), &Output::Score);
        assert_ne!(full.body, bare.body);
        let unknown = super::compile(&request(&[("profile", "polka")], SEED), &Output::Score);
        assert_eq!(unknown.status, 400);
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
        let presets: Vec<&str> = super::PRESETS.iter().map(|(name, _, _)| *name).collect();
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

    #[test]
    fn the_soundfont_shim_is_served_as_a_module() {
        // The import map points `@strudel/core` and `@strudel/webaudio` here; a 404 or a text
        // MIME type would leave every soundfont unregistered and the melody silent.
        let html = page().unwrap();
        assert!(
            html.contains("\"@strudel/webaudio\": \"/strudel-globals.mjs\""),
            "import map"
        );
        let shim = include_str!("strudel-globals.mjs");
        for name in [
            "registerSound",
            "getAudioContext",
            "Pattern",
            "noteToMidi",
            "startPresetNote",
        ] {
            assert!(shim.contains(name), "{name}");
        }
    }

    #[test]
    fn every_preset_compiles() {
        for (name, _, text) in super::PRESETS {
            let response = super::compile(&request(&[], body_of(text)), &Output::Pattern);
            assert_eq!(response.status, 200, "{name}: {}", response.body);
        }
    }

    #[test]
    fn the_seed_poem_compiles() {
        // The page opens by compiling it, so a broken one would greet every visitor with an error.
        let rendered = super::render(
            SEED,
            &super::Output::Pattern,
            &Mappings::all(),
            Profile::Machine,
        )
        .expect("the seed poem");
        assert!(rendered.contains("stack("), "{rendered}");
    }
}
