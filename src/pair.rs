//! AC ↔ test pair detection (no network).
//!
//! Parses numbered ACs out of a PRD (`**AC1**: ...`) and, for each, finds
//! the test that claims to verify it by these heuristics (first match wins):
//!
//! 1. `tests/ac<N>_*.rs` filename match (today's autobuilder convention).
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

/// Parse numbered ACs out of a PRD's markdown body.
///
/// Recognizes lines of the form `- **AC1**: ...` / `**AC1**: ...` /
/// `**AC1** — ...`. The text runs from after the separator to the end of the
/// logical bullet (subsequent indented continuation lines are folded in).
#[must_use]
pub fn parse_acs(prd_body: &str) -> Vec<Ac> {
    // `**AC<digits>**` optionally followed by `:` or an em/en dash.
    let Ok(header) = Regex::new(r"\*\*AC(\d+)\*\*\s*[:—-]?\s*(.*)") else {
        return Vec::new();
    };
    let mut acs: Vec<Ac> = Vec::new();
    let mut current: Option<(String, u32, String)> = None;

    let flush = |acc: &mut Vec<Ac>, cur: Option<(String, u32, String)>| {
        if let Some((id, index, text)) = cur {
            acc.push(Ac {
                id,
                index,
                text: text.trim().to_owned(),
            });
        }
    };

    for line in prd_body.lines() {
        // Strip only leading list/whitespace markers, NOT `*` — the `**` that
        // delimits `**AC1**` is load-bearing for the header regex below.
        let stripped = line.trim_start_matches(['-', ' ', '\t']);
        if let Some(caps) = header.captures(stripped) {
            // Start of a new AC: flush the previous one.
            flush(&mut acs, current.take());
            let index: u32 = caps
                .get(1)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0);
            let rest = caps.get(2).map_or("", |m| m.as_str()).to_owned();
            current = Some((format!("AC{index}"), index, rest));
        } else if let Some((_, _, text)) = current.as_mut() {
            // Continuation line: fold non-blank lines into the current AC.
            let trimmed = line.trim();
            if trimmed.is_empty() {
                // Blank line ends the AC bullet.
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
/// Heuristics are applied in PRD order; the first match wins.
fn find_test_for(index: u32, test_files: &[PathBuf]) -> io::Result<Option<PathBuf>> {
    // Heuristic 1: tests/ac<N>_*.rs
    let prefix1 = format!("ac{index}_");
    for path in test_files {
        if file_stem(path).is_some_and(|s| s.starts_with(&prefix1)) {
            return Ok(Some(path.clone()));
        }
    }
    // Heuristic 2: tests/acceptance_ac<N>.rs
    let stem2 = format!("acceptance_ac{index}");
    for path in test_files {
        if file_stem(path).is_some_and(|s| s == stem2) {
            return Ok(Some(path.clone()));
        }
    }
    // Heuristic 3: #[test] fn ac<N>_... in any test file.
    let fn_marker = format!("fn ac{index}_");
    for path in test_files {
        let body = fs::read_to_string(path)?;
        if body.contains(&fn_marker) {
            return Ok(Some(path.clone()));
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
        assert_eq!(acs[1].id, "AC2");
    }

    #[test]
    fn folds_continuation_lines() {
        let body = "- **AC1**: starts here\n  and continues here.\n\n- **AC2**: next.\n";
        let acs = parse_acs(body);
        assert_eq!(acs.len(), 2);
        assert_eq!(acs[0].text, "starts here and continues here.");
    }
}
