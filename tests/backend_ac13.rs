//! Acceptance test for PRD-ac-judge-pluggable-backend AC13.
//!
//! AC13 — Given the calibrate golden set and a codex stub, when `ac-judge
//! calibrate --golden-set <dir>` runs, then it completes on the codex
//! backend and reports the confusion matrix as before.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod support;

use std::process::Command;

#[test]
fn ac13_calibrate_completes_on_codex_backend() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("good_01.json"),
        r#"{"ac_text":"output ends with the cut bytes","test_source":"assert_eq!(last4, [0x1D,0x56,0x42,0x00]);","label":"good"}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("bad_01.json"),
        r#"{"ac_text":"the thing happens","test_source":"let g = thing(); assert_eq!(g, thing());","label":"bad"}"#,
    )
    .unwrap();

    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let out = Command::new(bin)
        .args(["calibrate", "--golden-set"])
        .arg(root)
        .env_remove("ANTHROPIC_API_KEY")
        .env("AC_JUDGE_CODEX_BIN", support::codex_stub_path())
        .env("STUB_VERDICT", support::PASSING_VERDICT)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("backend=codex"),
        "calibrate must resolve + announce the codex backend; got: {stderr}"
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("FPR=") && stdout.contains("FNR="),
        "calibrate must report the confusion matrix; got: {stdout}"
    );
}
