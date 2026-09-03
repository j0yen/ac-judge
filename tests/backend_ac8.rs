//! Acceptance test for PRD-ac-judge-pluggable-backend AC8.
//!
//! AC8 — Given a backend stub that returns text that is not the strict JSON
//! verdict, when `run` executes, then the run exits non-zero with a "bad
//! response" message naming the AC and no passing receipt is written.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod support;

use std::process::Command;

#[test]
fn ac8_nonjson_reply_is_a_hard_failure_not_a_silent_partial() {
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
        .env("STUB_NONJSON", "1")
        .output()
        .unwrap();

    assert_ne!(
        out.status.code(),
        Some(0),
        "a non-JSON reply must not be treated as a pass"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("bad response"),
        "stderr must say 'bad response'; got: {stderr}"
    );
    assert!(
        stderr.contains("AC1"),
        "stderr must name the failing AC; got: {stderr}"
    );

    assert!(
        !root
            .join("target/autobuilder/ac-semantic-judge.json")
            .exists(),
        "no receipt should be written when a backend reply cannot be parsed"
    );
}
