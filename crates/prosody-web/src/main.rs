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
use prosody_emit::style::{BANKS, SYNTHS};
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
fn page() -> String {
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

    include_str!("index.html")
        .replace("__SCALES__", &list(&scales))
        .replace("__BANKS__", &list(BANKS))
        .replace("__SYNTHS__", &list(SYNTHS))
        .replace("__MAPPINGS__", &format!("[{}]", mappings.join(",")))
        .replace("__SEED_POEM__", &json_string(SEED))
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

    let rendered = page();
    server::serve(port, move |request| {
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/") => Response::html(rendered.clone()),
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

/// The style header the controls describe.
fn header(request: &Request) -> String {
    let value = |name: &str, fallback: &str| request.param(name).unwrap_or(fallback).to_owned();
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

    format!(
        "+++\n[style]\nscale = \"{}\"\nroot = \"{}\"\ntempo_range = [{low}, {high}]\n\
         swing = {}\nbank = \"{}\"\nsynth = \"{}\"\n+++\n",
        value("scale", "minor"),
        value("root", "C"),
        f64::from(number("swing", 0).min(100)) / 100.0,
        value("bank", "tr909"),
        value("synth", "triangle"),
    )
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
        let html = page();
        for scale in prosody_emit::Scale::ALL {
            assert!(html.contains(scale.name()), "{} missing", scale.name());
        }
        for name in prosody_emit::style::BANKS
            .iter()
            .chain(prosody_emit::style::SYNTHS)
        {
            assert!(html.contains(name), "{name} missing");
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
        let html = page();
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
