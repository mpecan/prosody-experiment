//! Prosody Tauri application library.

use prosody_web::{Request, Site, Transport};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

static SITE: OnceLock<Site> = OnceLock::new();

/// An HTTP-like response from the native compiler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Answer {
    /// HTTP status code: 200, 400, or 404.
    pub status: u16,
    /// Response body: JSON or error message.
    pub body: String,
}

/// Answers a compilation request natively on device.
#[tauri::command]
fn respond(target: String, body: String) -> Result<Answer, String> {
    let site = SITE.get_or_init(|| {
        Site::new(Transport::Server).expect("failed to initialize native prosody site")
    });
    let response = site.respond(&Request::new("POST", &target, body));
    Ok(Answer {
        status: response.status,
        body: response.body,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .invoke_handler(tauri::generate_handler![respond])
        .run(tauri::generate_context!())
        .expect("error while running prosody application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_respond_compiles_poem() {
        let answer = respond(
            "/pattern".to_string(),
            "Tyger Tyger, burning bright,\n".to_string(),
        )
        .unwrap();
        assert_eq!(answer.status, 200);
        assert!(answer.body.contains("stack("), "body: {}", answer.body);
    }
}
