//! Acceptance test for PRD-ac-judge-pluggable-backend AC2.
//!
//! AC2 — Given no codex binary and `$ANTHROPIC_API_KEY` set with
//! `AC_JUDGE_API_ENDPOINT` pointing at a local stub server, when `run`
//! executes with default flags, then the request reaches the stub with
//! `x-api-key`, and the receipt has `"backend": "api"`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod support;

use std::fs;
use std::process::Command;

#[test]
fn ac2_falls_back_to_api_backend_without_codex() {
    let (_dir, prd) = support::one_ac_fixture();
    let root = prd.parent().unwrap();
    let server = support::StubApiServer::start(support::PASSING_VERDICT);

    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let out = Command::new(bin)
        .args(["run", "--prd"])
        .arg(&prd)
        .arg("--crate-root")
        .arg(root)
        .env("ANTHROPIC_API_KEY", "sk-test-not-real")
        .env("AC_JUDGE_API_ENDPOINT", server.endpoint())
        .env("AC_JUDGE_CODEX_BIN", support::missing_bin())
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("backend=api"),
        "stderr must announce backend=api; got: {stderr}"
    );
    assert!(
        server.saw_api_key(),
        "the request must have carried x-api-key"
    );

    let receipt = fs::read_to_string(root.join("target/autobuilder/ac-semantic-judge.json"))
        .expect("receipt written");
    let v: serde_json::Value = serde_json::from_str(&receipt).unwrap();
    assert_eq!(v["backend"], "api");
}
