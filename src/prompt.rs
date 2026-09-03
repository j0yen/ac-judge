//! Shared system text + per-AC user content, plus Anthropic request assembly.
//!
//! [`SYSTEM_TEXT`] and [`build_user_content`] are the words every backend
//! sends — codex and claude-cli receive them as-is (system, blank line, user
//! content on stdin / as the prompt); the `api` backend additionally wraps
//! them into an Anthropic Messages request via [`build_request_from_parts`],
//! whose system block is marked `cache_control: {"type": "ephemeral"}` so it
//! is prompt-cached across the many per-AC calls in one run.

use serde_json::{Value, json};

/// The cached system instruction block (~1500 input tokens with few-shot).
pub const SYSTEM_TEXT: &str = "You are an independent reviewer judging whether a Rust test \
exercises the behavior its acceptance criterion describes. You will receive (1) the AC's \
English text from a PRD, and (2) the full source of the test that claims to verify it.\n\n\
Answer in strict JSON only:\n\n\
{\"behavior_match\": \"yes\" | \"no\" | \"partial\", \"assertion_kind\": \"asserts-invariant\" \
| \"restates-impl\" | \"mixed\", \"confidence\": 0.0..1.0, \"reasoning\": \"<1-2 sentences>\"}\n\n\
\"asserts-invariant\" means the test asserts a property the AC's English describes (e.g., \
\"output ends with cut bytes\" -> asserts the last bytes are 0x1D 0x56 0x42 0x00). \
\"restates-impl\" means the test calls the function and asserts the function returned what the \
function returned (tautological).";

/// Build the per-AC user-message content: AC English text + test source.
///
/// Every backend sends exactly these words as its "user" turn (system text
/// is [`SYSTEM_TEXT`], shared verbatim), so a verdict differs across
/// backends only because of the model, not the prompt.
#[must_use]
pub fn build_user_content(
    ac_text: &str,
    test_source: &str,
    crate_name: &str,
    ac_index: u32,
) -> String {
    format!(
        "Crate: {crate_name}\nAcceptance criterion: AC{ac_index}\n\nAC English text:\n{ac_text}\n\n\
Test source:\n```rust\n{test_source}\n```\n\nReturn the strict JSON verdict only."
    )
}

/// Build the JSON request body for the Anthropic Messages API for one AC.
///
/// `max_tokens` is small because the model must reply with only the strict
/// JSON verdict.
#[must_use]
pub fn build_request(
    model: &str,
    ac_text: &str,
    test_source: &str,
    crate_name: &str,
    ac_index: u32,
) -> Value {
    let user_content = build_user_content(ac_text, test_source, crate_name, ac_index);
    build_request_from_parts(model, SYSTEM_TEXT, &user_content)
}

/// Build the JSON request body for the Anthropic Messages API from an
/// already-assembled system + user turn.
///
/// Used by the `api` backend, which receives `system`/`user` through the
/// [`crate::backend::Backend`] trait rather than the raw AC/test parts.
#[must_use]
pub fn build_request_from_parts(model: &str, system: &str, user_content: &str) -> Value {
    json!({
        "model": model,
        "max_tokens": 512,
        "system": [
            {
                "type": "text",
                "text": system,
                "cache_control": { "type": "ephemeral" }
            }
        ],
        "messages": [
            {
                "role": "user",
                "content": user_content
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_carries_ephemeral_cache_control_on_system() {
        let req = build_request("claude-sonnet-4-6", "ac text", "fn t() {}", "demo", 1);
        let cc = &req["system"][0]["cache_control"]["type"];
        assert_eq!(cc, "ephemeral");
    }

    #[test]
    fn user_content_includes_ac_and_test() {
        let req = build_request("m", "the invariant", "assert_eq!(1, 1);", "demo", 3);
        let content = req["messages"][0]["content"].as_str().unwrap();
        assert!(content.contains("the invariant"));
        assert!(content.contains("assert_eq!(1, 1);"));
        assert!(content.contains("AC3"));
    }
}
