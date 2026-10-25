//! `prosody-wasm` — the whole compiler, inside the page.
//!
//! The page asks four things of whatever answers it: a pattern, the annotated poem, the
//! measurements, and the reading's cues ([`prosody_web::Site`]). Served by `prosody-web`, they are
//! HTTP requests. Built for `wasm32-unknown-unknown` and bound with `wasm-bindgen`, this crate
//! answers the same requests with the same code in the browser, so the page can be hosted as static
//! files and nobody has to install anything to hear a poem.
//!
//! # Determinism
//!
//! Invariant I1 promises bit-identical output for the same poem and style, and a second target is
//! a new way to break it. Nothing here touches a float or a clock: the text is made by the same
//! formatting code as everywhere else. `scripts/check-wasm.sh` compares this build's answers with
//! the native server's, byte for byte.

use prosody_web::{Request, Site, Transport};
use wasm_bindgen::prelude::wasm_bindgen;

thread_local! {
    /// Made once: its data carries every preset and option list.
    static SITE: Option<Site> = Site::new(Transport::Wasm).ok();
}

/// What the server would have sent back: an HTTP status and the body.
#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    status: u16,
    body: String,
}

#[wasm_bindgen]
impl Answer {
    /// The HTTP status: 200, or 400 with the reason in the body, or 404.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn status(&self) -> u16 {
        self.status
    }

    /// The body: the view asked for, or why it could not be made.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn body(&self) -> String {
        self.body.clone()
    }
}

/// Answers a `POST` to `target` — a path and query, `/pattern?scale=lydian` — with the poem as the
/// body, exactly as the page's server would.
#[wasm_bindgen]
#[must_use]
pub fn respond(target: &str, body: &str) -> Answer {
    SITE.with(|site| match site {
        Some(site) => {
            let response = site.respond(&Request::new("POST", target, body.to_owned()));
            Answer {
                status: response.status,
                body: response.body,
            }
        }
        None => Answer {
            status: 500,
            body: "the page could not be prepared\n".to_owned(),
        },
    })
}

// Tests may unwrap and index freely; a panic in a test is a test failure. See CLAUDE.md §3.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::respond;

    const COUPLET: &str = "the fire will burn the night\nand turn the dark to light\n";

    #[test]
    fn it_answers_what_the_server_answers() {
        // Same library, so the same bytes; what this checks is the binding around it — the target
        // split into path and query, and the status and body handed back whole.
        let site = prosody_web::Site::new(prosody_web::Transport::Server).unwrap();
        for target in [
            "/pattern?scale=lydian",
            "/explain?set=bass:feet",
            "/reading",
            "/score?profile=jazz",
        ] {
            let served = site.respond(&prosody_web::Request::new(
                "POST",
                target,
                COUPLET.to_owned(),
            ));
            let answer = respond(target, COUPLET);
            assert_eq!(answer.status(), served.status, "{target}");
            assert_eq!(answer.body(), served.body, "{target}");
        }
    }

    #[test]
    fn a_refusal_comes_back_with_its_reason() {
        let answer = respond("/pattern?off=polka", COUPLET);
        assert_eq!(answer.status(), 400);
        assert!(answer.body().contains("unknown mapping `polka`"));
        assert_eq!(respond("/nowhere", COUPLET).status(), 404);
    }

    #[test]
    fn a_poem_in_any_script_survives_the_trip() {
        let answer = respond("/explain", "Zdravljica — naprej, zastava slave!\n");
        assert_eq!(answer.status(), 200, "{}", answer.body());
        assert!(answer.body().contains("zastava"));
    }
}
