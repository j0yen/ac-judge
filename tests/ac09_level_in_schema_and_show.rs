//! Acceptance test for PRD-ac-judge-numbered-ac-format AC9.
//!
//! AC9 — Given a receipt produced from a numbered-form PRD, When validated
//! against the updated `schemas/ac-semantic-judge.schema.json`, Then it
//! passes and each verdict has a `level` string; and `ac-judge show --slug
//! AC1` prints that level.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::option_if_let_else
)]

mod support;

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::Value;

/// Load the shipped schema next to this crate.
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

/// Same tiny hand-rolled subset-of-JSON-Schema validator as
/// `tests/acceptance_ac9.rs` (required, additionalProperties, enum,
/// pattern), duplicated locally rather than shared: these two files belong
/// to different PRDs and `tests/support` is deliberately test-binary-scoped,
/// not schema-scoped.
fn validate(instance: &Value, schema: &Value, defs: &Value) {
    let schema = if let Some(r) = schema.get("$ref").and_then(Value::as_str) {
        let name = r.rsplit('/').next().unwrap();
        &defs[name]
    } else {
        schema
    };
    if let Some(en) = schema.get("enum").and_then(Value::as_array) {
        assert!(en.contains(instance), "value {instance} not in enum {en:?}");
    }
    if let Some(pat) = schema.get("pattern").and_then(Value::as_str) {
        let s = instance.as_str().expect("string for pattern");
        let ok = match pat {
            "^AC[0-9]+$" => s
                .strip_prefix("AC")
                .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())),
            "^P[0-2]$" => matches!(s, "P0" | "P1" | "P2"),
            other => panic!("unhandled test pattern {other}"),
        };
        assert!(ok, "value {s:?} does not match pattern {pat}");
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
fn ac09_receipt_from_numbered_prd_validates_and_show_prints_level() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("tests/ac01_basic.rs"),
        "#[test]\nfn ac1_x() { assert!(true); }\n",
    )
    .unwrap();
    let prd = root.join("PRD.md");
    std::fs::write(
        &prd,
        "## Acceptance criteria\n\n1. P0 — the thing happens.\n",
    )
    .unwrap();

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
    assert_eq!(
        run_out.status.code(),
        Some(0),
        "run should pass: stderr {}",
        String::from_utf8_lossy(&run_out.stderr)
    );

    // Validate the written receipt against the shipped schema.
    let receipt_path = root
        .join("target")
        .join("autobuilder")
        .join("ac-semantic-judge.json");
    let receipt_body = std::fs::read_to_string(&receipt_path).unwrap();
    let receipt: Value = serde_json::from_str(&receipt_body).unwrap();
    let schema = load_schema();
    let defs = schema["$defs"].clone();
    validate(&receipt, &schema, &defs);

    let ac1 = &receipt["verdicts"][0];
    assert_eq!(
        ac1["level"], "P0",
        "verdict for a numbered-form AC must carry its level"
    );

    // `ac-judge show --slug AC1` prints the level in its header.
    let show_out = Command::new(bin)
        .args(["show", "--slug", "AC1", "--crate-root"])
        .arg(root)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&show_out.stdout);
    assert!(
        stdout
            .lines()
            .next()
            .is_some_and(|l| l.contains("level=P0")),
        "show header must print the level; got: {stdout}"
    );
}
