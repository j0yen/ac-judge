//! Integration fixtures for PRD-ac-judge-numbered-ac-format: realistic
//! documents under `tests/fixtures/`, parsed end to end through
//! `ac_judge::pair::parse_acs`.
//!
//! `prd-numbered/` is a pure `/build`-contract PRD (AC1/AC2 guardrail: the
//! numbered form parses correctly and numbered lines outside the acceptance
//! section — under `## Requirements` / `## Success metrics` — are excluded).
//! `prd-mixed/` simulates an agorabus-era PRD that later grew a `/build`
//! contract section too (AC5 guardrail: union-by-index, first-occurrence
//! wins, sorted by index — the identical-AC-list-before/after regression
//! the PRD's success metrics call out).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::similar_names
)]

use std::path::PathBuf;

use ac_judge::pair::parse_acs;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .join("PRD.md");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn prd_numbered_fixture_parses_only_the_acceptance_section() {
    let body = fixture("prd-numbered");
    let acs = parse_acs(&body);

    assert_eq!(
        acs.len(),
        3,
        "the ## Requirements and ## Success metrics numbered lines must not \
         be picked up as ACs; got {acs:?}"
    );
    assert_eq!(acs[0].id, "AC1");
    assert_eq!(acs[0].level, Some("P0".to_owned()));
    assert!(acs[0].text.contains("handshake completes within 100ms"));
    assert_eq!(acs[1].id, "AC2");
    assert_eq!(acs[2].id, "AC3");
    assert_eq!(acs[2].level, Some("P1".to_owned()));
}

#[test]
fn prd_mixed_fixture_unions_by_index_first_occurrence_wins_sorted() {
    let body = fixture("prd-mixed");
    let acs = parse_acs(&body);

    // AC1, AC2, AC3 (bullets) + AC4 (numbered-only) = 4 ACs, sorted.
    assert_eq!(acs.len(), 4, "got {acs:?}");
    let indices: Vec<u32> = acs.iter().map(|a| a.index).collect();
    assert_eq!(indices, vec![1, 2, 3, 4], "must be sorted by index");

    // AC2's bullet-form text (earlier in the document) wins over its
    // numbered-form text (later in the document).
    let ac2 = &acs[1];
    assert!(
        ac2.text.contains("bullet-form text for AC2"),
        "AC2 must keep the first-occurring (bullet) text; got {:?}",
        ac2.text
    );
    assert_eq!(
        ac2.level, None,
        "the winning form (bullet) carries no level"
    );

    // AC4 only exists in the numbered form and carries its level.
    let ac4 = &acs[3];
    assert_eq!(ac4.level, Some("P1".to_owned()));
}
