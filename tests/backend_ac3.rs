//! Acceptance test for PRD-ac-judge-pluggable-backend AC3.
//!
//! AC3 — Given no codex binary, no API key, and `AC_JUDGE_CLAUDE_BIN`
//! pointing at a stub that prints a `claude -p` JSON envelope, when `run`
//! executes, then the reply is taken from `.result`, the receipt has
//! `"backend": "claude-cli"`, and the stub received `--tools ""` and
//! `--max-turns 1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod support;

use std::fs;
use std::process::Command;

#[test]
fn ac3_falls_back_to_claude_cli_backend_last() {
    let (dir, prd) = support::one_ac_fixture();
    let root = prd.parent().unwrap();
    let argv_file = dir.path().join("claude_argv.txt");

    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let out = Command::new(bin)
        .args(["run", "--prd"])
        .arg(&prd)
        .arg("--crate-root")
        .arg(root)
        .env_remove("ANTHROPIC_API_KEY")
        .env("AC_JUDGE_CODEX_BIN", support::missing_bin())
        .env("AC_JUDGE_CLAUDE_BIN", support::claude_stub_path())
        .env("STUB_VERDICT", support::PASSING_VERDICT)
        .env("STUB_ARGV_FILE", &argv_file)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("backend=claude-cli"),
        "stderr must announce backend=claude-cli; got: {stderr}"
    );

    let receipt = fs::read_to_string(root.join("target/autobuilder/ac-semantic-judge.json"))
        .expect("receipt written");
    let v: serde_json::Value = serde_json::from_str(&receipt).unwrap();
    assert_eq!(v["backend"], "claude-cli");
    // The verdict was successfully taken from .result and parsed: a passing
    // stub verdict means the (only) AC did not fail the gate.
    assert_eq!(v["verdicts"][0]["behavior_match"], "yes");

    let argv = fs::read_to_string(&argv_file).expect("stub recorded argv");
    let flags: Vec<&str> = argv.lines().collect();
    assert!(
        flags.windows(2).any(|w| w == ["--tools", ""]),
        "claude must be invoked with --tools \"\"; argv: {flags:?}"
    );
    assert!(
        flags.windows(2).any(|w| w == ["--max-turns", "1"]),
        "claude must be invoked with --max-turns 1; argv: {flags:?}"
    );
}
