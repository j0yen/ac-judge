//! Acceptance test for PRD-ac-judge-pluggable-backend AC6.
//!
//! AC6 — Given `--backend codex` and no usable codex, when `run` executes
//! with a valid API key present, then it exits 6 naming `codex` and never
//! calls the API stub.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod support;

use std::process::Command;

#[test]
fn ac6_explicit_backend_never_substitutes() {
    let (_dir, prd) = support::one_ac_fixture();
    let root = prd.parent().unwrap();
    let server = support::StubApiServer::start(support::PASSING_VERDICT);

    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let out = Command::new(bin)
        .args(["run", "--prd"])
        .arg(&prd)
        .arg("--crate-root")
        .arg(root)
        .arg("--backend")
        .arg("codex")
        .env("ANTHROPIC_API_KEY", "sk-test-not-real")
        .env("AC_JUDGE_API_ENDPOINT", server.endpoint())
        .env("AC_JUDGE_CODEX_BIN", support::missing_bin())
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(6),
        "explicit --backend codex must not fall back to api"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("codex"),
        "diagnostic must name codex; got: {stderr}"
    );
    assert!(
        !stderr.contains("api:"),
        "must not report on the api backend at all; got: {stderr}"
    );
    assert_eq!(
        server.request_count(),
        0,
        "the api stub must never have been called"
    );
}
