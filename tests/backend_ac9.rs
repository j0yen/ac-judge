//! Acceptance test for PRD-ac-judge-pluggable-backend AC9.
//!
//! AC9 — Given a codex stub that sleeps longer than the per-call deadline,
//! when `run` executes with the deadline set to a test-sized value via
//! `AC_JUDGE_CALL_TIMEOUT_SECS`, then the call fails with `transport error:
//! timeout` within that window and the child process is gone.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod support;

use std::fs;
use std::process::Command;
use std::time::Instant;

#[test]
fn ac9_per_call_deadline_kills_a_hung_backend() {
    let (_dir, prd) = support::one_ac_fixture();
    let root = prd.parent().unwrap();

    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let start = Instant::now();
    let out = Command::new(bin)
        .args(["run", "--prd"])
        .arg(&prd)
        .arg("--crate-root")
        .arg(root)
        .env_remove("ANTHROPIC_API_KEY")
        .env("AC_JUDGE_CODEX_BIN", support::codex_stub_path())
        .env("STUB_SLEEP", "30")
        .env("AC_JUDGE_CALL_TIMEOUT_SECS", "1")
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    // The whole run (spawn + 1s deadline + kill/reap + receipt write) must
    // complete well inside the stub's 30s sleep — proof the child was
    // actually killed rather than waited out (a live, un-reaped child would
    // otherwise keep this process (and cargo test's harness) blocked on it).
    assert!(
        elapsed.as_secs() < 10,
        "timeout must be enforced promptly; took {elapsed:?}"
    );
    assert!(
        out.status.success(),
        "a single timed-out AC degrades to a partial verdict, not a run failure"
    );

    let receipt = fs::read_to_string(root.join("target/autobuilder/ac-semantic-judge.json"))
        .expect("receipt written");
    let v: serde_json::Value = serde_json::from_str(&receipt).unwrap();
    let reasoning = v["verdicts"][0]["reasoning"].as_str().unwrap_or_default();
    assert!(
        reasoning.contains("transport error: timeout"),
        "the verdict must record the exact timeout error; got: {reasoning}"
    );
}
