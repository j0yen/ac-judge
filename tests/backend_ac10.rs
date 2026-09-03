//! Acceptance test for PRD-ac-judge-pluggable-backend AC10.
//!
//! AC10 — Given a receipt produced by any backend, when validated against
//! `schemas/ac-semantic-judge.schema.json`, then it passes, and `ac-judge
//! show --slug AC1` prints a header containing the backend and model.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod support;

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::Value;

fn load_schema() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/schemas/ac-semantic-judge.schema.json"
    );
    let body = std::fs::read_to_string(path).expect("schema file present in repo");
    serde_json::from_str(&body).expect("schema is valid JSON")
}

fn keys(v: &Value) -> BTreeSet<String> {
    v.as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

/// Minimal recursive validator covering the subset of JSON-Schema this
/// schema actually uses (required / additionalProperties / enum / $ref),
/// mirroring the one in `tests/acceptance_ac9.rs`.
fn validate(instance: &Value, schema: &Value, defs: &Value) {
    let schema = schema
        .get("$ref")
        .and_then(Value::as_str)
        .map_or(schema, |r| {
            let name = r.rsplit('/').next().unwrap();
            &defs[name]
        });

    if let Some(en) = schema.get("enum").and_then(Value::as_array) {
        assert!(en.contains(instance), "value {instance} not in enum {en:?}");
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => {
            if let Some(req) = schema.get("required").and_then(Value::as_array) {
                let present = keys(instance);
                for r in req {
                    let r = r.as_str().unwrap();
                    assert!(present.contains(r), "missing required key {r}");
                }
            }
            if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                let allowed: BTreeSet<String> = schema["properties"]
                    .as_object()
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                for k in keys(instance) {
                    assert!(allowed.contains(&k), "unexpected additional property {k}");
                }
            }
            if let Some(props) = schema.get("properties").and_then(Value::as_object) {
                for (k, sub) in props {
                    if let Some(child) = instance.get(k) {
                        validate(child, sub, defs);
                    }
                }
            }
        }
        Some("array") => {
            if let Some(item_schema) = schema.get("items") {
                for item in instance.as_array().expect("array") {
                    validate(item, item_schema, defs);
                }
            }
        }
        _ => {}
    }
}

#[test]
fn ac10_receipt_validates_and_show_prints_backend_and_model() {
    let (_dir, prd) = support::one_ac_fixture();
    let root = prd.parent().unwrap();

    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let run_out = Command::new(bin)
        .args(["run", "--prd"])
        .arg(&prd)
        .arg("--crate-root")
        .arg(root)
        .env_remove("ANTHROPIC_API_KEY")
        .env("AC_JUDGE_CODEX_BIN", support::codex_stub_path())
        .env("STUB_VERDICT", support::PASSING_VERDICT)
        .output()
        .unwrap();
    assert!(
        run_out.status.success(),
        "run must pass; stderr: {}",
        String::from_utf8_lossy(&run_out.stderr)
    );

    let receipt_body =
        std::fs::read_to_string(root.join("target/autobuilder/ac-semantic-judge.json")).unwrap();
    let receipt: Value = serde_json::from_str(&receipt_body).unwrap();

    let schema = load_schema();
    let defs = schema["$defs"].clone();
    validate(&receipt, &schema, &defs);

    let show_out = Command::new(bin)
        .args(["show", "--slug", "AC1", "--crate-root"])
        .arg(root)
        .output()
        .unwrap();
    assert!(
        show_out.status.success(),
        "show must succeed; stderr: {}",
        String::from_utf8_lossy(&show_out.stderr)
    );
    let stdout = String::from_utf8_lossy(&show_out.stdout);
    assert!(
        stdout.contains("backend=codex"),
        "show header must contain the backend; got: {stdout}"
    );
    assert!(
        stdout.contains("model="),
        "show header must contain the model; got: {stdout}"
    );
}
