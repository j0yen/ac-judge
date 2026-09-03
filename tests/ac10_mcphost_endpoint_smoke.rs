//! Acceptance test for PRD-ac-judge-numbered-ac-format AC10.
//!
//! AC10 — Given `/home/jsy/Documents/PRDs/build-queue/PRD-mcphost-endpoint.md`
//! and `--crate-root /home/jsy/wintermute/mcphost` with `--backend auto`,
//! When `ac-judge run` executes on `RedBaron`, Then it reports 19 ACs, at
//! least 16 paired, and exits 0 or 4 — never 2.
//!
//! This test reaches outside the crate to the fleet's PRD queue and the
//! sibling `mcphost` checkout, following the same "reaches outside the
//! crate" pattern as `backend_ac12.rs` (ac-judge-pluggable-backend). It
//! points `AC_JUDGE_CODEX_BIN` at the hermetic stub so the run never depends
//! on a live network call or a logged-in `codex` CLI: on a machine whose
//! mcphost checkout already carries a warm `target/autobuilder/ac-judge-cache`
//! from a prior real judge run, those cache hits are served as-is; on a
//! fresh checkout the stub's canned passing verdict is used instead. Either
//! way the assertions below only depend on the AC count and the structural
//! pairing (which is backend-independent), never on verdict content.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stderr
)]

mod support;

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn ac10_mcphost_endpoint_reports_19_acs_at_least_16_paired_never_exit_2() {
    let prd = PathBuf::from("/home/jsy/Documents/PRDs/build-queue/PRD-mcphost-endpoint.md");
    let crate_root = PathBuf::from("/home/jsy/wintermute/mcphost");
    if !prd.is_file() || !crate_root.is_dir() {
        eprintln!(
            "ac10: skipping — fleet paths not present on this machine ({} / {})",
            prd.display(),
            crate_root.display()
        );
        return;
    }

    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let out = Command::new(bin)
        .args(["run", "--prd"])
        .arg(&prd)
        .arg("--crate-root")
        .arg(&crate_root)
        .arg("--backend")
        .arg("auto")
        .env("AC_JUDGE_CODEX_BIN", support::codex_stub_path())
        .env("STUB_LOGIN_OK", "1")
        .env("STUB_VERDICT", support::PASSING_VERDICT)
        .output()
        .expect("run ac-judge");

    let code = out.status.code().unwrap();
    assert!(
        code == 0 || code == 4,
        "exit must be 0 or 4, never 2; got {code}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let receipt_path = crate_root.join("target/autobuilder/ac-semantic-judge.json");
    let receipt = fs::read_to_string(&receipt_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", receipt_path.display()));
    let v: serde_json::Value = serde_json::from_str(&receipt).expect("receipt is valid JSON");
    let verdicts = v["verdicts"].as_array().expect("verdicts array");

    assert_eq!(
        verdicts.len(),
        19,
        "expected 19 ACs on PRD-mcphost-endpoint.md; got {}",
        verdicts.len()
    );

    let paired = verdicts
        .iter()
        .filter(|verdict| verdict.get("test_path").and_then(|t| t.as_str()).is_some())
        .count();
    assert!(
        paired >= 16,
        "expected at least 16 of 19 ACs paired; got {paired}"
    );
}
