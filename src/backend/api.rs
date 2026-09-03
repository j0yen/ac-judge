//! `api` backend: sync Anthropic Messages client (`ureq`).
//!
//! This is the original (v0.1) transport, unchanged except that it now lives
//! behind the [`super::Backend`] trait. Endpoint overridable by
//! `$AC_JUDGE_API_ENDPOINT` so tests never touch the real network.

use std::env;

use super::{Backend, Check, Error};

/// The environment variable holding the Anthropic API key.
pub const API_KEY_ENV: &str = "ANTHROPIC_API_KEY";

/// The environment variable overriding the Anthropic Messages endpoint, for
/// tests.
pub const ENDPOINT_ENV: &str = "AC_JUDGE_API_ENDPOINT";

/// The Anthropic Messages endpoint used when `$AC_JUDGE_API_ENDPOINT` is
/// unset.
pub const DEFAULT_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";

/// The `api` backend: one Anthropic Messages API key, one model.
pub struct ApiBackend {
    api_key: String,
    model: String,
}

/// Build the `api` backend if `$ANTHROPIC_API_KEY` is set and non-blank.
///
/// # Errors
///
/// Returns a [`Check`] naming `api` with outcome `"key unset"` when the
/// variable is absent or blank.
pub fn build(model_override: Option<&str>) -> Result<ApiBackend, Check> {
    let api_key = match env::var(API_KEY_ENV) {
        Ok(k) if !k.trim().is_empty() => k,
        _ => {
            return Err(Check {
                name: "api",
                outcome: "key unset".to_owned(),
            });
        }
    };
    let model = model_override.unwrap_or(crate::DEFAULT_MODEL).to_owned();
    Ok(ApiBackend { api_key, model })
}

fn endpoint() -> String {
    env::var(ENDPOINT_ENV).unwrap_or_else(|_| DEFAULT_ENDPOINT.to_owned())
}

impl Backend for ApiBackend {
    fn name(&self) -> &'static str {
        "api"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn judge(&self, system: &str, user: &str) -> Result<String, Error> {
        let body = crate::prompt::build_request_from_parts(&self.model, system, user);
        let body_str =
            serde_json::to_string(&body).map_err(|e| Error::BadResponse(e.to_string()))?;

        let response_text = ureq::post(&endpoint())
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", "2023-06-01")
            .set("content-type", "application/json")
            .set("anthropic-beta", "prompt-caching-2024-07-31")
            .send_string(&body_str)
            .map_err(|e| Error::Transport(e.to_string()))
            .and_then(|resp| {
                resp.into_string()
                    .map_err(|e| Error::Transport(e.to_string()))
            })?;

        extract_content_text(&response_text)
            .ok_or_else(|| Error::BadResponse(format!("no text content in: {response_text}")))
    }
}

/// Extract `content[0].text` from an Anthropic Messages API response.
fn extract_content_text(response_body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(response_body).ok()?;
    v.get("content")?
        .as_array()?
        .iter()
        .find(|block| block.get("type").and_then(|t| t.as_str()) == Some("text"))
        .and_then(|block| block.get("text"))
        .and_then(|t| t.as_str())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_content_text_finds_text_block() {
        let body = r#"{"content":[{"type":"text","text":"{\"ok\":true}"}]}"#;
        assert_eq!(
            extract_content_text(body).as_deref(),
            Some(r#"{"ok":true}"#)
        );
    }

    #[test]
    fn extract_content_text_none_when_absent() {
        assert_eq!(extract_content_text(r#"{"content":[]}"#), None);
        assert_eq!(extract_content_text("not json"), None);
    }
}
