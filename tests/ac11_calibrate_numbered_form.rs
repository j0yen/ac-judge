//! Acceptance test for PRD-ac-judge-numbered-ac-format AC11.
//!
//! AC11 — Given the `calibrate` golden set rewritten in the numbered form,
//! When `ac-judge calibrate --golden-set <dir>` runs against a stub backend,
//! Then it produces the same confusion matrix as with the bullet form.
//!
//! The stub backend replies with a fixed verdict regardless of what it's
//! shown, so this proves the thing this AC actually cares about: rewriting
//! `ac_text` from plain English to the bullet form to the numbered form
//! (all normalized by `pair::extract_ac_text`, see AC9 in the PRD's
//! requirements) does not change the confusion matrix the run produces.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::path::Path;
use std::process::Command;

/// Write a two-pair golden set (one good, one bad) under `dir`, with
/// `ac_text` written in the given form.
fn write_golden_set(dir: &Path, form: Form) {
    let good_text = match form {
        Form::Plain => "output ends with the cut bytes".to_owned(),
        Form::Bullet => "**AC1**: output ends with the cut bytes".to_owned(),
        Form::Numbered => "1. P0 — output ends with the cut bytes".to_owned(),
    };
    let bad_text = match form {
        Form::Plain => "the thing happens".to_owned(),
        Form::Bullet => "**AC2**: the thing happens".to_owned(),
        Form::Numbered => "2. P0 — the thing happens".to_owned(),
    };
    std::fs::write(
        dir.join("good_01.json"),
        serde_json::json!({
            "ac_text": good_text,
            "test_source": "assert_eq!(last4, [0x1D,0x56,0x42,0x00]);",
            "label": "good"
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.join("bad_01.json"),
        serde_json::json!({
            "ac_text": bad_text,
            "test_source": "let g = thing(); assert_eq!(g, thing());",
            "label": "bad"
        })
        .to_string(),
    )
    .unwrap();
}

#[derive(Clone, Copy)]
enum Form {
    Plain,
    Bullet,
    Numbered,
}

fn run_calibrate(dir: &Path) -> String {
    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let out = Command::new(bin)
        .args(["calibrate", "--golden-set"])
        .arg(dir)
        .env_remove("ANTHROPIC_API_KEY")
        .env("AC_JUDGE_CODEX_BIN", support::codex_stub_path())
        .env("STUB_VERDICT", support::PASSING_VERDICT)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success() || out.status.code() == Some(4),
        "calibrate must complete the confusion-matrix gate (pass or fail on \
         the rate thresholds, not error out); stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
}

/// Pull the `FPR=... FNR=... tp=... fp=... tn=... fn=...` line out of
/// `calibrate`'s stdout, dropping the preceding per-pair progress noise.
fn confusion_line(stdout: &str) -> &str {
    stdout
        .lines()
        .find(|l| l.contains("FPR="))
        .expect("calibrate must print the confusion-matrix line")
}

#[test]
fn ac11_numbered_golden_set_matches_bullet_and_plain_forms() {
    let plain_dir = tempfile::tempdir().unwrap();
    write_golden_set(plain_dir.path(), Form::Plain);
    let plain_out = run_calibrate(plain_dir.path());

    let bullet_dir = tempfile::tempdir().unwrap();
    write_golden_set(bullet_dir.path(), Form::Bullet);
    let bullet_out = run_calibrate(bullet_dir.path());

    let numbered_dir = tempfile::tempdir().unwrap();
    write_golden_set(numbered_dir.path(), Form::Numbered);
    let numbered_out = run_calibrate(numbered_dir.path());

    let plain_line = confusion_line(&plain_out);
    let bullet_line = confusion_line(&bullet_out);
    let numbered_line = confusion_line(&numbered_out);

    assert_eq!(
        plain_line, bullet_line,
        "plain and bullet forms must produce the same confusion matrix"
    );
    assert_eq!(
        plain_line, numbered_line,
        "plain and numbered forms must produce the same confusion matrix"
    );
}
