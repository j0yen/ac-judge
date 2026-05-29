//! System + few-shot Anthropic request assembly.
//!
//! The system block is marked `cache_control: {"type": "ephemeral"}` so it is
//! prompt-cached across the many per-AC calls in one run. The ephemeral
//! per-AC content (AC text + test source) goes in the user message.

use serde_json::{json, Value};

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

/// Build the JSON request body for the Anthropic Messages API for one AC.
///
/// `max_tokens` is small because the model must reply with only the strict
/// JSON verdict.
#[must_use]
pub fn build_request(model: &str, ac_text: &str, test_source: &str, crate_name: &str, ac_index: u32) -> Value {
    let user_content = format!(
        "Crate: {crate_name}\nAcceptance criterion: AC{ac_index}\n\nAC English text:\n{ac_text}\n\n\
Test source:\n```rust\n{test_source}\n```\n\nReturn the strict JSON verdict only."
    );
    json!({
        "model": model,
        "max_tokens": 512,
        "system": [
            {
                "type": "text",
                "text": SYSTEM_TEXT,
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
