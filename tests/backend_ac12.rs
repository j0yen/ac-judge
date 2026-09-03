//! Acceptance test for PRD-ac-judge-pluggable-backend AC12.
//!
//! AC12 — Given the rustbuild skill checkout at `~/wintermute/rustbuild`,
//! when `grep -n 'codex login' skill/SKILL.md` runs, then step 11's
//! requirement line matches, and `grep -c 'ANTHROPIC_API_KEY (exits 6'
//! skill/SKILL.md` is 0.
//!
//! This test reaches outside the crate to the fleet's shared rustbuild skill
//! checkout, so it is inherently environment-dependent rather than hermetic
//! — the same way AC12 itself is (it names a fixed path on the operator's
//! machine). It is skipped, not failed, when that checkout isn't present
//! (e.g. a bare `cargo test` on a box without the wintermute tree).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use std::path::PathBuf;

fn skill_md_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home).join("wintermute/rustbuild/skill/SKILL.md");
    path.is_file().then_some(path)
}

#[test]
fn ac12_rustbuild_skill_doc_names_all_three_backends() {
    let Some(path) = skill_md_path() else {
        eprintln!(
            "ac12: ~/wintermute/rustbuild/skill/SKILL.md not present on this host — skipping"
        );
        return;
    };
    let body = std::fs::read_to_string(&path).expect("read SKILL.md");

    assert!(
        body.contains("codex login"),
        "SKILL.md step 11 must name `codex login` as a way to satisfy the judge-backend requirement"
    );
    assert_eq!(
        body.matches("ANTHROPIC_API_KEY (exits 6").count(),
        0,
        "the old key-only exit-6 sentence must be gone from SKILL.md"
    );
}
