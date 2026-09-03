//! Acceptance test for PRD-ac-judge-pluggable-backend AC11.
//!
//! AC11 — Given the same AC/test pair judged under two different backends,
//! when the cache directory is inspected, then two distinct cache files
//! exist (backend is part of the key).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod support;

use std::fs;
use std::process::Command;

#[test]
fn ac11_cache_is_partitioned_by_backend() {
    let (_dir, prd) = support::one_ac_fixture();
    let root = prd.parent().unwrap();
    let cache_dir = root.join("target/autobuilder/ac-judge-cache");

    let bin = env!("CARGO_BIN_EXE_ac-judge");

    let codex_out = Command::new(bin)
        .args(["run", "--prd"])
        .arg(&prd)
        .arg("--crate-root")
        .arg(root)
        .env_remove("ANTHROPIC_API_KEY")
        .env("AC_JUDGE_CODEX_BIN", support::codex_stub_path())
        .env("STUB_VERDICT", support::PASSING_VERDICT)
        .output()
        .unwrap();
    assert!(
        codex_out.status.success(),
        "codex run must pass; stderr: {}",
        String::from_utf8_lossy(&codex_out.stderr)
    );
    let count_after_codex = fs::read_dir(&cache_dir).unwrap().count();
    assert_eq!(count_after_codex, 1, "one cache file after the codex run");

    let claude_out = Command::new(bin)
        .args(["run", "--prd"])
        .arg(&prd)
        .arg("--crate-root")
        .arg(root)
        .arg("--backend")
        .arg("claude-cli")
        .env_remove("ANTHROPIC_API_KEY")
        .env("AC_JUDGE_CLAUDE_BIN", support::claude_stub_path())
        .env("STUB_VERDICT", support::PASSING_VERDICT)
        .output()
        .unwrap();
    assert!(
        claude_out.status.success(),
        "claude-cli run must pass; stderr: {}",
        String::from_utf8_lossy(&claude_out.stderr)
    );

    assert_eq!(
        fs::read_dir(&cache_dir).unwrap().count(),
        2,
        "the same AC/test pair judged by two backends must produce two distinct cache files"
    );
}
