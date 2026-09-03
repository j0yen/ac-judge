//! AC ↔ test pair detection (no network).
//!
//! Parses acceptance criteria out of a PRD in either of two forms:
//!
//! 1. The `**AC1**: ...` bullet form (agorabus / episodic-observer era),
//!    recognized anywhere in the document.
//! 2. The `/build` contract's numbered form — `1. P0 — ...` — recognized
//!    only inside a section whose heading matches `## Acceptance`,
//!    `## Acceptance criteria`, or `## Acceptance tests` (case-insensitive).
//!    Numbered lines elsewhere (Requirements, Success metrics, ...) are
//!    never ACs.
//!
//! Both forms fold indented continuation lines into the AC's text. When an
//! index appears in both forms, the one that occurs first in the document
//! wins; the final list is deduped by index and sorted by index.
//!
//! For each parsed AC, [`pair_all`] finds the test that claims to verify it
//! by these heuristics (first match wins), all accepting indices zero-padded
//! to width 1-3 (`ac1_`, `ac01_`, `ac001_`), anchored so `ac1_` never matches
//! `ac10_`:
//!
//! 1. `tests/ac<N>_*.rs` filename match (rustbuild's scaffold convention).
//! 2. `tests/acceptance_ac<N>.rs` match (older agorabus / episodic-observer
//!    convention).
//! 3. A `#[test]` function whose name starts with `ac<N>_` in any test file.
//! 4. Falls through to `unpaired` if no match.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use regex::Regex;

/// A declared acceptance criterion parsed out of a PRD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac {
    /// The AC identifier, e.g. `AC1`.
    pub id: String,
    /// The numeric index, e.g. `1` for `AC1`.
    pub index: u32,
    /// The AC's declared priority level (`P0`/`P1`/`P2`), when the source
    /// line carried one. Only the numbered contract form (`N. P0 — ...`)
    /// carries a level; the `**AC<N>**: ...` bullet form never does.
    pub level: Option<String>,
    /// The English text of the AC, with surrounding whitespace trimmed.
    pub text: String,
}

/// The result of pairing one AC to a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    /// The acceptance criterion.
    pub ac: Ac,
    /// Path to the paired test file, or `None` when unpaired.
    pub test_path: Option<PathBuf>,
}

/// Fold `lines` into ACs using `header` to recognize the start of a new AC
/// (after `strip`ping any leading marker irrelevant to the header match) and
/// the shared continuation rule: a blank line or a `##`-heading line ends
/// the current AC; any other non-blank line is folded into its text, joined
/// by single spaces. Returns `(source line number, Ac)` pairs so callers can
/// resolve "which form occurred first" across multiple passes.
fn fold_acs<'a>(
    lines: impl Iterator<Item = (usize, &'a str)>,
    strip: impl Fn(&'a str) -> &'a str,
    header: impl Fn(&str) -> Option<(u32, Option<String>, String)>,
) -> Vec<(usize, Ac)> {
    fn flush(acc: &mut Vec<(usize, Ac)>, cur: Option<(usize, u32, Option<String>, String)>) {
        if let Some((line_no, index, level, text)) = cur {
            acc.push((
                line_no,
                Ac {
                    id: format!("AC{index}"),
                    index,
                    level,
                    text: text.trim().to_owned(),
                },
            ));
        }
    }

    let mut acs: Vec<(usize, Ac)> = Vec::new();
    let mut current: Option<(usize, u32, Option<String>, String)> = None;

    for (line_no, line) in lines {
        let stripped = strip(line);
        if let Some((index, level, rest)) = header(stripped) {
            // Start of a new AC: flush the previous one.
            flush(&mut acs, current.take());
            current = Some((line_no, index, level, rest));
        } else if let Some((_, _, _, text)) = current.as_mut() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                flush(&mut acs, current.take());
            } else if !trimmed.starts_with("##") {
                text.push(' ');
                text.push_str(trimmed);
            } else {
                flush(&mut acs, current.take());
            }
        }
    }
    flush(&mut acs, current);
    acs
}

/// Pass 1: the `**AC1**: ...` bullet form, recognized anywhere in the
/// document.
fn parse_bullet_form(prd_body: &str) -> Vec<(usize, Ac)> {
    // `**AC<digits>**` optionally followed by `:` or an em/en dash.
    let Ok(header) = Regex::new(r"\*\*AC(\d+)\*\*\s*[:—-]?\s*(.*)") else {
        return Vec::new();
    };
    fold_acs(
        prd_body.lines().enumerate(),
        // Strip only leading list/whitespace markers, NOT `*` — the `**`
        // that delimits `**AC1**` is load-bearing for the header regex.
        |line| line.trim_start_matches(['-', ' ', '\t']),
        |stripped| {
            header.captures(stripped).map(|caps| {
                let index: u32 = caps
                    .get(1)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(0);
                let rest = caps.get(2).map_or("", |m| m.as_str()).to_owned();
                (index, None, rest)
            })
        },
    )
}

/// Whether `line` is a level-2 markdown heading (`## Foo`). A level-3+
/// heading (`### Foo`) does not count — it does not end an acceptance
/// section entered by a level-2 heading.
const fn is_level2_heading(line: &str) -> bool {
    let bytes = line.as_bytes();
    matches!(bytes, [b'#', b'#', c, ..] if c.is_ascii_whitespace())
}

/// Collect `(line number, line)` for every line that falls inside a section
/// whose heading matches `^##\s+Acceptance(\s+criteria|\s+tests)?\s*$`
/// (case-insensitive). A section ends at the next level-2 heading or EOF;
/// heading lines themselves are excluded from the output.
fn acceptance_section_lines(prd_body: &str) -> Vec<(usize, &str)> {
    let Ok(acceptance_heading) = Regex::new(r"(?i)^##\s+Acceptance(\s+criteria|\s+tests)?\s*$")
    else {
        return Vec::new();
    };
    let mut in_section = false;
    let mut out = Vec::new();
    for (line_no, line) in prd_body.lines().enumerate() {
        let trimmed = line.trim_end();
        if acceptance_heading.is_match(trimmed) {
            in_section = true;
            continue;
        }
        if is_level2_heading(trimmed) {
            in_section = false;
            continue;
        }
        if in_section {
            out.push((line_no, line));
        }
    }
    out
}

/// Pass 2: the `/build` contract's numbered form, recognized only inside an
/// acceptance-criteria section.
fn parse_numbered_form(prd_body: &str) -> Vec<(usize, Ac)> {
    let Ok(numbered) = Regex::new(r"^\s*(\d+)\.\s+(?:(P[0-2])\s*[—–-]\s*)?(.+)$") else {
        return Vec::new();
    };
    let lines = acceptance_section_lines(prd_body);
    fold_acs(
        lines.into_iter(),
        |line| line,
        |line| {
            numbered.captures(line).map(|caps| {
                let index: u32 = caps
                    .get(1)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(0);
                let level = caps.get(2).map(|m| m.as_str().to_owned());
                let text = caps.get(3).map_or("", |m| m.as_str()).to_owned();
                (index, level, text)
            })
        },
    )
}

/// Parse acceptance criteria out of a PRD's markdown body.
///
/// Runs both the bullet-form and numbered-form passes (see the module docs)
/// and unions the results by index: when the same index appears in both
/// forms, whichever occurred first in the document wins. The result is
/// deduped by index and sorted by index.
#[must_use]
pub fn parse_acs(prd_body: &str) -> Vec<Ac> {
    let mut combined = parse_bullet_form(prd_body);
    combined.extend(parse_numbered_form(prd_body));
    combined.sort_by_key(|(line_no, _)| *line_no);

    let mut seen = std::collections::HashSet::new();
    let mut acs: Vec<Ac> = combined
        .into_iter()
        .filter(|(_, ac)| seen.insert(ac.index))
        .map(|(_, ac)| ac)
        .collect();
    acs.sort_by_key(|ac| ac.index);
    acs
}

/// Extract `(level, text)` from a single AC's raw text.
///
/// Recognizes both forms so a golden-set pair can be authored in either —
/// wraps `raw` in a synthetic `## Acceptance criteria` section and calls
/// [`parse_acs`], so this is "the same parser" the PRD and bullet forms use.
/// Falls back to `raw` verbatim (trimmed, no level) when neither form
/// matches, which preserves the historical golden-set format: plain English
/// text with no marker at all.
#[must_use]
pub fn extract_ac_text(raw: &str) -> (Option<String>, String) {
    let trimmed = raw.trim();
    let wrapped = format!("## Acceptance criteria\n\n{trimmed}\n");
    if let Some(ac) = parse_acs(&wrapped).into_iter().next() {
        return (ac.level, ac.text);
    }
    (None, trimmed.to_owned())
}

/// List the `*.rs` files under `<crate_root>/tests/`, if the dir exists.
fn list_test_files(crate_root: &Path) -> io::Result<Vec<PathBuf>> {
    let tests_dir = crate_root.join("tests");
    if !tests_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&tests_dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// Find the test file for one AC index within an already-listed test set.
///
/// Heuristics are applied in PRD order; the first match wins. Every
/// heuristic accepts the index zero-padded to width 1-3 (`ac1_`, `ac01_`,
/// `ac001_`), anchored so `ac1_` never matches `ac10_` or `ac01x_nope.rs`.
fn find_test_for(index: u32, test_files: &[PathBuf]) -> io::Result<Option<PathBuf>> {
    // Heuristic 1: tests/ac<N>_*.rs (rustbuild's `ac01_*.rs` scaffold
    // convention).
    if let Ok(re1) = Regex::new(&format!(r"^ac0{{0,2}}{index}_")) {
        for path in test_files {
            if file_stem(path).is_some_and(|s| re1.is_match(s)) {
                return Ok(Some(path.clone()));
            }
        }
    }
    // Heuristic 2: tests/acceptance_ac<N>.rs (older agorabus /
    // episodic-observer convention).
    if let Ok(re2) = Regex::new(&format!(r"^acceptance_ac0{{0,2}}{index}$")) {
        for path in test_files {
            if file_stem(path).is_some_and(|s| re2.is_match(s)) {
                return Ok(Some(path.clone()));
            }
        }
    }
    // Heuristic 3: #[test] fn ac<N>_... in any test file.
    if let Ok(re3) = Regex::new(&format!(r"fn ac0{{0,2}}{index}_")) {
        for path in test_files {
            let body = fs::read_to_string(path)?;
            if re3.is_match(&body) {
                return Ok(Some(path.clone()));
            }
        }
    }
    Ok(None)
}

fn file_stem(path: &Path) -> Option<&str> {
    path.file_stem().and_then(|s| s.to_str())
}

/// Pair every AC parsed from `prd_body` to a test under `crate_root`.
///
/// # Errors
///
/// Returns an error if a test directory entry or test source file cannot be
/// read.
pub fn pair_all(prd_body: &str, crate_root: &Path) -> io::Result<Vec<Pair>> {
    let acs = parse_acs(prd_body);
    let test_files = list_test_files(crate_root)?;
    let mut pairs = Vec::with_capacity(acs.len());
    for ac in acs {
        let test_path = find_test_for(ac.index, &test_files)?;
        pairs.push(Pair { ac, test_path });
    }
    Ok(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_acs() {
        let body = "## Acceptance criteria\n\n- **AC1**: first thing.\n- **AC2**: second thing.\n";
        let acs = parse_acs(body);
        assert_eq!(acs.len(), 2);
        assert_eq!(acs[0].id, "AC1");
        assert_eq!(acs[0].index, 1);
        assert_eq!(acs[0].text, "first thing.");
        assert_eq!(acs[0].level, None);
        assert_eq!(acs[1].id, "AC2");
    }

    #[test]
    fn folds_continuation_lines() {
        let body = "- **AC1**: starts here\n  and continues here.\n\n- **AC2**: next.\n";
        let acs = parse_acs(body);
        assert_eq!(acs.len(), 2);
        assert_eq!(acs[0].text, "starts here and continues here.");
    }

    /// AC1 (PRD-ac-judge-numbered-ac-format) — the numbered contract form
    /// under a `## Acceptance criteria` heading is parsed with its level and
    /// text.
    #[test]
    fn ac1_parses_numbered_contract_form_with_level() {
        let body =
            "## Acceptance criteria\n\n1. P0 — Given a, When b, Then c\n2. P1 — second one\n";
        let acs = parse_acs(body);
        assert_eq!(acs.len(), 2);
        assert_eq!(acs[0].id, "AC1");
        assert_eq!(acs[0].index, 1);
        assert_eq!(acs[0].level, Some("P0".to_owned()));
        assert_eq!(acs[0].text, "Given a, When b, Then c");
        assert_eq!(acs[1].id, "AC2");
        assert_eq!(acs[1].level, Some("P1".to_owned()));
    }

    /// AC2 — numbered lines outside an acceptance section (e.g. under
    /// `## Requirements` or `## Success metrics`) are never ACs.
    #[test]
    fn ac2_numbered_lines_outside_section_are_not_acs() {
        let body = "## Requirements\n\n1. Do the first thing.\n2. Do the second thing.\n\n## Success metrics\n\n1. metric one\n";
        let acs = parse_acs(body);
        assert!(acs.is_empty());
    }

    /// AC3 — indented continuation lines directly following a numbered AC
    /// line fold into its text, joined by single spaces.
    #[test]
    fn ac3_numbered_form_folds_continuation_lines() {
        let body =
            "## Acceptance criteria\n\n1. P0 — starts here\n  and continues\n  across two lines.\n";
        let acs = parse_acs(body);
        assert_eq!(acs.len(), 1);
        assert_eq!(acs[0].text, "starts here and continues across two lines.");
    }

    /// AC4 — the `**AC1**: ...` fixture's output is unchanged from v0.2.0:
    /// same id/index/text, and now an explicit `level: None` (bullets never
    /// carry a level).
    #[test]
    fn ac4_bullet_form_regression_matches_v0_2_0() {
        let body = "## Acceptance criteria\n\n- **AC1**: first thing.\n- **AC2**: second thing.\n";
        let acs = parse_acs(body);
        assert_eq!(acs.len(), 2);
        assert_eq!(acs[0].id, "AC1");
        assert_eq!(acs[0].index, 1);
        assert_eq!(acs[0].text, "first thing.");
        assert_eq!(acs[0].level, None);
        assert_eq!(acs[1].id, "AC2");
        assert_eq!(acs[1].index, 2);
        assert_eq!(acs[1].text, "second thing.");
        assert_eq!(acs[1].level, None);
    }

    /// AC5 — when both forms declare the same index, the one that appears
    /// first in the document wins, and the result is sorted by index.
    #[test]
    fn ac5_dedup_by_index_first_occurrence_wins_sorted() {
        let body = "**AC2**: old\n\n## Acceptance criteria\n\n1. P0 — first\n2. P0 — new\n";
        let acs = parse_acs(body);
        assert_eq!(acs.len(), 2);
        // Sorted by index.
        assert_eq!(acs[0].index, 1);
        assert_eq!(acs[1].index, 2);
        // AC2's text is "old" (the bullet, which occurs earlier in the doc)
        // not "new" (the numbered form, which occurs later).
        assert_eq!(acs[1].text, "old");
    }

    /// AC6 — pairing heuristic 1 accepts zero-padded indices and is
    /// anchored so `ac1_` never matches `ac10_` or `ac1x_nope.rs`.
    #[test]
    fn ac6_heuristic1_zero_padded_and_anchored() {
        let dir = tempfile::tempdir().unwrap();
        let tests_dir = dir.path().join("tests");
        fs::create_dir_all(&tests_dir).unwrap();
        fs::write(tests_dir.join("ac01_signup.rs"), "// t\n").unwrap();
        fs::write(tests_dir.join("ac10_other.rs"), "// t\n").unwrap();
        fs::write(tests_dir.join("ac1x_nope.rs"), "// t\n").unwrap();

        let test_files = list_test_files(dir.path()).unwrap();

        let found1 = find_test_for(1, &test_files).unwrap();
        assert_eq!(
            found1.as_deref().and_then(Path::file_name),
            Some(std::ffi::OsStr::new("ac01_signup.rs"))
        );

        let found10 = find_test_for(10, &test_files).unwrap();
        assert_eq!(
            found10.as_deref().and_then(Path::file_name),
            Some(std::ffi::OsStr::new("ac10_other.rs"))
        );
    }

    /// AC7 — pairing heuristic 3 matches a zero-padded `fn ac<N>_...` marker
    /// inside any test file, even when the filename itself doesn't match
    /// heuristics 1 or 2.
    #[test]
    fn ac7_heuristic3_fn_marker_zero_padded() {
        let dir = tempfile::tempdir().unwrap();
        let tests_dir = dir.path().join("tests");
        fs::create_dir_all(&tests_dir).unwrap();
        fs::write(
            tests_dir.join("metering.rs"),
            "#[test]\nfn ac07_usage() { assert!(true); }\n",
        )
        .unwrap();

        let test_files = list_test_files(dir.path()).unwrap();
        let found = find_test_for(7, &test_files).unwrap();
        assert_eq!(
            found.as_deref().and_then(Path::file_name),
            Some(std::ffi::OsStr::new("metering.rs"))
        );
    }

    /// `extract_ac_text` normalizes all three golden-set authoring forms
    /// (plain, bullet, numbered) down to the same clean text.
    #[test]
    fn extract_ac_text_normalizes_all_forms() {
        assert_eq!(
            extract_ac_text("output ends with the cut bytes"),
            (None, "output ends with the cut bytes".to_owned())
        );
        assert_eq!(
            extract_ac_text("**AC1**: output ends with the cut bytes"),
            (None, "output ends with the cut bytes".to_owned())
        );
        assert_eq!(
            extract_ac_text("1. P0 — output ends with the cut bytes"),
            (
                Some("P0".to_owned()),
                "output ends with the cut bytes".to_owned()
            )
        );
    }
}
