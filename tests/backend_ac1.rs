//! Acceptance test for PRD-ac-judge-pluggable-backend AC1.
//!
//! AC1 — Given `AC_JUDGE_CODEX_BIN` points at a stub whose `login status`
//! exits 0 and `$ANTHROPIC_API_KEY` is unset, when `ac-judge run` runs with
//! default flags, then stderr contains `backend=codex`, the receipt has
//! `"backend": "codex"`, and the exit code is 0 or 4 exactly as the stub's
//! verdicts dictate.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod support;

use std::fs;
use std::process::Command;

#[test]
fn ac1_codex_is_default_backend_when_available() {
    let (_dir, prd) = support::one_ac_fixture();
    let root = prd.parent().unwrap();

    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let out = Command::new(bin)
        .args(["run", "--prd"])
        .arg(&prd)
        .arg("--crate-root")
        .arg(root)
        .env_remove("ANTHROPIC_API_KEY")
        .env("AC_JUDGE_CODEX_BIN", support::codex_stub_path())
        .env("STUB_LOGIN_OK", "1")
        .env("STUB_VERDICT", support::PASSING_VERDICT)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("backend=codex"),
        "stderr must announce backend=codex; got: {stderr}"
    );

    let code = out.status.code().unwrap();
    assert!(
        code == 0 || code == 4,
        "exit must be 0 or 4 per the stub's verdict; got {code}, stderr: {stderr}"
    );

    let receipt = fs::read_to_string(root.join("target/autobuilder/ac-semantic-judge.json"))
        .expect("receipt written");
    let v: serde_json::Value = serde_json::from_str(&receipt).unwrap();
    assert_eq!(v["backend"], "codex");
}
