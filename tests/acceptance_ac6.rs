//! Acceptance test for AC6 (MUST) — golden-set calibration.
//!
//! Project: ac-judge (cli)
//! AC description: `ac-judge calibrate --golden-set golden/` against the 20
//! hand-curated pairs yields false-positive < 0.10 AND false-negative < 0.20.
//! The confusion-matrix gate is network-bound (it runs the judge against each
//! pair) and is deferred to the network iteration; this test verifies the
//! offline portion that lands this iteration: the shipped golden set loads,
//! carries the PRD-specified label distribution (10 good / 5 bad / 5 partial),
//! and the `calibrate` command parses + validates it without a network call.
//!
//! Ownership split: this //! header block (AC metadata) is read-only — to
//! change an AC, file agent/intent_card_amendment_request.json and re-scaffold.
//! The `fn acceptance_ac6` body BELOW is owned by the edit-agent.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::doc_markdown,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::as_conversions,
    clippy::cognitive_complexity,
    clippy::option_if_let_else,
    clippy::float_cmp,
    clippy::float_arithmetic
)]

use std::path::PathBuf;
use std::process::Command;

use ac_judge::calibrate::{Label, LabelCounts, load_golden_set};

/// Path to the shipped golden set, relative to the crate root.
fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden")
}

/// AC6 — the shipped golden set has the PRD-specified distribution and the
/// loader validates every pair offline.
#[test]
fn acceptance_ac6_golden_set_distribution() {
    let pairs = load_golden_set(&golden_dir()).expect("golden set loads");
    assert_eq!(pairs.len(), 20, "PRD requires 20 hand-curated pairs");

    let counts = LabelCounts::tally(&pairs);
    assert_eq!(counts.good, 10, "PRD requires 10 known-good pairs");
    assert_eq!(counts.bad, 5, "PRD requires 5 known-bad pairs");
    assert_eq!(counts.partial, 5, "PRD requires 5 partial pairs");
    assert_eq!(counts.total(), 20);

    // Every pair must carry non-empty AC text and test source so the judge has
    // something to score; ids must be unique for the confusion matrix.
    let mut ids = std::collections::HashSet::new();
    for p in &pairs {
        assert!(!p.ac_text.trim().is_empty(), "{}: empty ac_text", p.id);
        assert!(
            !p.test_source.trim().is_empty(),
            "{}: empty test_source",
            p.id
        );
        assert!(ids.insert(p.id.clone()), "duplicate golden id {}", p.id);
        // Label round-trips through the parser.
        let s = match p.label {
            Label::Good => "good",
            Label::Bad => "bad",
            Label::Partial => "partial",
        };
        assert_eq!(Label::parse(s).unwrap(), p.label);
    }
}

/// AC6 — the `calibrate` subcommand loads + validates the golden set offline
/// (no judge backend available, so no network call may be attempted) and
/// reports the count summary. The confusion-matrix gate itself is deferred
/// (exit 3) until a backend is available. `codex`/`claude` are pointed at
/// nonexistent paths so this is hermetic regardless of what is installed and
/// logged in on the host (RedBaron carries both).
#[test]
fn acceptance_ac6_calibrate_command_validates_offline() {
    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let out = Command::new(bin)
        .args(["calibrate", "--golden-set"])
        .arg(golden_dir())
        .env_remove("ANTHROPIC_API_KEY")
        .env("AC_JUDGE_CODEX_BIN", "/nonexistent/ac-judge-test-codex")
        .env("AC_JUDGE_CLAUDE_BIN", "/nonexistent/ac-judge-test-claude")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("good=10") && stdout.contains("bad=5") && stdout.contains("partial=5"),
        "calibrate must report the loaded distribution; stdout: {stdout}"
    );
    // Offline portion succeeds; the network confusion-matrix gate is deferred.
    assert_eq!(
        out.status.code(),
        Some(3),
        "calibrate offline-validates then defers the network gate (exit 3); stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// AC6 — a malformed golden set (bad label) is rejected with exit 2, not
/// silently passed.
#[test]
fn acceptance_ac6_rejects_malformed_golden_pair() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("oops.json"),
        r#"{"ac_text":"x","test_source":"y","label":"???"}"#,
    )
    .unwrap();

    let bin = env!("CARGO_BIN_EXE_ac-judge");
    let out = Command::new(bin)
        .args(["calibrate", "--golden-set"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "malformed golden pair must be rejected; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
