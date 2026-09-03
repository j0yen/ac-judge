//! Acceptance test for PRD-ac-judge-numbered-ac-format AC8.
//!
//! AC8 — Given a PRD with neither the `**AC<N>**:` bullet form nor the
//! numbered `N. P0 —` form under a `## Acceptance criteria` heading, When
//! `ac-judge run` executes, Then it exits 2 and the diagnostic on stderr
//! names both accepted forms (`**AC<N>**` and `N. P0 —`) and the section
//! heading requirement (`## Acceptance criteria`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::process::Command;

#[test]
fn ac08_exit2_message_names_both_forms_and_heading() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let prd = root.join("PRD.md");
    std::fs::write(
        &prd,
        "# PRD — no ACs here\n\n## Requirements\n\n1. just a requirement, not an AC.\n",
    )
    .unwrap();

    // Backend resolution happens before the AC check (AC4/AC6 of the prior
    // pluggable-backend PRD), so a codex stub is wired up to resolve
    // hermetically regardless of what's logged in on the host — the goal
    // here is just to reach the "no ACs found" diagnostic.
    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let out = Command::new(bin)
        .args(["run", "--prd"])
        .arg(&prd)
        .arg("--crate-root")
        .arg(root)
        .env_remove("ANTHROPIC_API_KEY")
        .env("AC_JUDGE_CODEX_BIN", support::codex_stub_path())
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(2),
        "no-ACs-found must exit 2; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("**AC<N>**"),
        "message must name the bullet form; got: {stderr}"
    );
    assert!(
        stderr.contains("N. P0 —"),
        "message must name the numbered form; got: {stderr}"
    );
    assert!(
        stderr.contains("## Acceptance criteria"),
        "message must name the section heading requirement; got: {stderr}"
    );
}
