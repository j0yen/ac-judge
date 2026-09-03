//! Acceptance test for PRD-ac-judge-pluggable-backend AC4.
//!
//! AC4 — Given no codex, no key, and no claude binary, when `run` executes,
//! then it exits 6, stderr lists all three checks with their outcome, and no
//! file is written under `target/autobuilder/`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod support;

use std::process::Command;

#[test]
fn ac4_exits_six_with_three_way_diagnostic_when_nothing_available() {
    let (_dir, prd) = support::one_ac_fixture();
    let root = prd.parent().unwrap();

    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let out = Command::new(bin)
        .args(["run", "--prd"])
        .arg(&prd)
        .arg("--crate-root")
        .arg(root)
        .env_remove("ANTHROPIC_API_KEY")
        .env("AC_JUDGE_CODEX_BIN", support::missing_bin())
        .env("AC_JUDGE_CLAUDE_BIN", support::missing_bin())
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(6),
        "no backend available must exit 6"
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("codex"),
        "diagnostic must mention codex; got: {stderr}"
    );
    assert!(
        stderr.contains("api"),
        "diagnostic must mention api; got: {stderr}"
    );
    assert!(
        stderr.contains("claude"),
        "diagnostic must mention claude; got: {stderr}"
    );
    assert!(
        stderr.contains("key unset"),
        "api check outcome must be reported; got: {stderr}"
    );
    assert!(
        stderr.contains("not on PATH"),
        "codex/claude check outcome must be reported; got: {stderr}"
    );

    assert!(
        !root.join("target/autobuilder").exists(),
        "no file may be written under target/autobuilder/ on the exit-6 path"
    );
}
