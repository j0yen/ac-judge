//! Sync Anthropic client (`ureq`) plus the verdict cache key.
//!
//! Network calls are deferred to a later autobuilder iteration; this module
//! ships the request-sending surface and the SHA-keyed cache contract so the
//! no-network ACs (key presence, cache key) can be proven now.

use std::env;

use sha2::{Digest, Sha256};

use crate::schema::Verdict;

/// The environment variable holding the Anthropic API key.
pub const API_KEY_ENV: &str = "ANTHROPIC_API_KEY";

/// The Anthropic Messages endpoint.
pub const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";

/// Errors the client surface can return.
#[derive(Debug)]
pub enum Error {
    /// `$ANTHROPIC_API_KEY` is unset or empty.
    MissingApiKey,
    /// The HTTP transport failed.
    Transport(String),
    /// The response body could not be parsed into a verdict.
    BadResponse(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingApiKey => write!(f, "${API_KEY_ENV} is unset"),
            Self::Transport(m) => write!(f, "transport error: {m}"),
            Self::BadResponse(m) => write!(f, "bad response: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// Read the API key from the environment, returning [`Error::MissingApiKey`]
/// if it is unset or empty. No network call is implied.
///
/// # Errors
///
/// Returns [`Error::MissingApiKey`] when the variable is absent or blank.
pub fn require_api_key() -> Result<String, Error> {
    match env::var(API_KEY_ENV) {
        Ok(k) if !k.trim().is_empty() => Ok(k),
        _ => Err(Error::MissingApiKey),
    }
}

/// Compute the verdict cache key:
/// `sha256(ac_text + test_source + model + prompt_version)`.
#[must_use]
pub fn cache_key(ac_text: &str, test_source: &str, model: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ac_text.as_bytes());
    hasher.update(test_source.as_bytes());
    hasher.update(model.as_bytes());
    hasher.update(crate::PROMPT_VERSION.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Parse the model's strict-JSON reply text into a [`Verdict`] for `ac_id`.
///
/// The model is instructed to emit only the verdict JSON; this also tolerates
/// the verdict being wrapped in a fenced code block or surrounded by prose.
///
/// # Errors
///
/// Returns [`Error::BadResponse`] if no JSON object with the expected fields
/// can be extracted.
pub fn parse_verdict(ac_id: &str, test_path: Option<String>, reply_text: &str) -> Result<Verdict, Error> {
    #[derive(serde::Deserialize)]
    struct Raw {
        behavior_match: crate::schema::BehaviorMatch,
        assertion_kind: crate::schema::AssertionKind,
        confidence: f64,
        reasoning: String,
    }
    let json_slice = extract_json_object(reply_text)
        .ok_or_else(|| Error::BadResponse("no JSON object in reply".to_owned()))?;
    let raw: Raw = serde_json::from_str(json_slice).map_err(|e| Error::BadResponse(e.to_string()))?;
    Ok(Verdict {
        ac_id: ac_id.to_owned(),
        test_path,
        behavior_match: raw.behavior_match,
        assertion_kind: raw.assertion_kind,
        confidence: raw.confidence,
        reasoning: raw.reasoning,
    })
}

/// Extract the first balanced `{...}` JSON object from arbitrary text.
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    for (offset, ch) in text[start..].char_indices() {
        if in_str {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return text.get(start..=start + offset);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{AssertionKind, BehaviorMatch};

    #[test]
    fn cache_key_is_stable_and_sensitive() {
        let a = cache_key("ac", "test", "m");
        let b = cache_key("ac", "test", "m");
        let c = cache_key("ac", "test2", "m");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn parses_verdict_with_surrounding_prose() {
        let reply = "Here is my verdict:\n```json\n{\"behavior_match\": \"yes\", \
\"assertion_kind\": \"asserts-invariant\", \"confidence\": 0.9, \"reasoning\": \"ok\"}\n```";
        let v = parse_verdict("AC1", Some("tests/ac1_x.rs".to_owned()), reply).unwrap();
        assert_eq!(v.behavior_match, BehaviorMatch::Yes);
        assert_eq!(v.assertion_kind, AssertionKind::AssertsInvariant);
        assert!((v.confidence - 0.9).abs() < 1e-9);
    }
}
