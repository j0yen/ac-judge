//! Acceptance test for PRD-ac-judge-pluggable-backend AC5.
//!
//! AC5 — Given a codex stub whose `login status` exits non-zero, when `run`
//! executes with `--backend auto` and a claude stub present, then the run
//! proceeds on `claude-cli` and stderr says so.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod support;

use std::process::Command;

#[test]
fn ac5_codex_logged_out_falls_through_to_claude_cli() {
    let (_dir, prd) = support::one_ac_fixture();
    let root = prd.parent().unwrap();

    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let out = Command::new(bin)
        .args(["run", "--prd"])
        .arg(&prd)
        .arg("--crate-root")
        .arg(root)
        .arg("--backend")
        .arg("auto")
        .env_remove("ANTHROPIC_API_KEY")
        .env("AC_JUDGE_CODEX_BIN", support::codex_stub_path())
        .env("STUB_LOGIN_OK", "0")
        .env("AC_JUDGE_CLAUDE_BIN", support::claude_stub_path())
        .env("STUB_VERDICT", support::PASSING_VERDICT)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("backend=claude-cli"),
        "a logged-out codex must fall through to claude-cli, and stderr must say so; got: {stderr}"
    );
    assert!(
        !stderr.contains("backend=codex"),
        "must not have selected codex; got: {stderr}"
    );
}
