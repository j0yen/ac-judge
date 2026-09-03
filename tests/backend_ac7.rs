//! Acceptance test for PRD-ac-judge-pluggable-backend AC7.
//!
//! AC7 — Given a first run completed on any backend, when the same command
//! runs again with the stub instrumented to count invocations, then the
//! count is zero and the verdicts are identical.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod support;

use std::fs;
use std::process::Command;

#[test]
fn ac7_second_run_is_a_pure_cache_hit() {
    let (dir, prd) = support::one_ac_fixture();
    let root = prd.parent().unwrap();
    let count_file = dir.path().join("codex_invocations.txt");

    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let run_once = || {
        Command::new(bin)
            .args(["run", "--prd"])
            .arg(&prd)
            .arg("--crate-root")
            .arg(root)
            .env_remove("ANTHROPIC_API_KEY")
            .env("AC_JUDGE_CODEX_BIN", support::codex_stub_path())
            .env("STUB_VERDICT", support::PASSING_VERDICT)
            .env("STUB_COUNT_FILE", &count_file)
            .output()
            .unwrap()
    };

    let first = run_once();
    assert!(
        first.status.success(),
        "first run must pass; stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let calls_after_first = fs::read_to_string(&count_file)
        .unwrap_or_default()
        .lines()
        .count();
    assert_eq!(
        calls_after_first, 1,
        "first run must call the backend exactly once"
    );
    let receipt_first =
        fs::read_to_string(root.join("target/autobuilder/ac-semantic-judge.json")).unwrap();

    let second = run_once();
    assert!(
        second.status.success(),
        "second run must pass; stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let calls_after_second = fs::read_to_string(&count_file)
        .unwrap_or_default()
        .lines()
        .count();
    assert_eq!(
        calls_after_second, 1,
        "second run must be a pure cache hit — zero additional backend calls"
    );

    let receipt_second =
        fs::read_to_string(root.join("target/autobuilder/ac-semantic-judge.json")).unwrap();
    let v1: serde_json::Value = serde_json::from_str(&receipt_first).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&receipt_second).unwrap();
    assert_eq!(
        v1["verdicts"], v2["verdicts"],
        "cached verdicts must be identical across runs"
    );
}
