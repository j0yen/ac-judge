//! Golden-set calibration runner.
//!
//! `ac-judge calibrate --golden-set <dir>` runs the judge against
//! hand-curated AC-↔-test pairs labeled good/bad/partial and reports the
//! confusion matrix. The CI gate is false-positive `< 0.10` and
//! false-negative `< 0.20`.
//!
//! The judge call against each golden pair is network-bound and deferred to a
//! later iteration; the confusion-matrix math and the golden-set loader below
//! are pure and tested now.

use std::fs;
use std::path::Path;

use serde::Deserialize;

/// The expected label for a golden pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    /// The test genuinely verifies its AC.
    Good,
    /// The test fails the gate (restates-impl or no behavior match).
    Bad,
    /// The test partially verifies its AC.
    Partial,
}

impl Label {
    /// Parse a label from its on-disk string form.
    ///
    /// # Errors
    /// Returns `Err` with the offending token when the string is not one of
    /// `good` / `bad` / `partial` (case-insensitive).
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "good" => Ok(Self::Good),
            "bad" => Ok(Self::Bad),
            "partial" => Ok(Self::Partial),
            other => Err(format!("unknown label {other:?} (expected good|bad|partial)")),
        }
    }
}

/// One hand-curated AC-↔-test pair from the golden set.
///
/// Each pair lives in its own `*.json` file under the golden-set directory and
/// carries the AC's English text, the full source of the paired test, and the
/// human label. The judge is run against `ac_text` + `test_source`; the verdict
/// is then scored against `label` to fill the confusion matrix.
#[derive(Debug, Clone, Deserialize)]
pub struct GoldenPair {
    /// Stable identifier for the pair (filename stem when not set in the file).
    #[serde(default)]
    pub id: String,
    /// The AC's English text from a PRD.
    pub ac_text: String,
    /// The full source of the paired test.
    pub test_source: String,
    /// The human label, deserialized via [`Label::parse`].
    #[serde(deserialize_with = "de_label")]
    pub label: Label,
}

fn de_label<'de, D>(d: D) -> Result<Label, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(d)?;
    Label::parse(&s).map_err(serde::de::Error::custom)
}

/// Load every `*.json` golden pair under `dir`, sorted by filename for
/// determinism. A pair's `id` defaults to its filename stem when the file
/// omits one.
///
/// # Errors
/// Returns `Err` if the directory cannot be read, a file is not valid JSON, or
/// a file carries an unknown label.
pub fn load_golden_set(dir: &Path) -> Result<Vec<GoldenPair>, String> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("cannot read golden-set dir {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    entries.sort();

    let mut pairs = Vec::with_capacity(entries.len());
    for path in entries {
        let body = fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let mut pair: GoldenPair = serde_json::from_str(&body)
            .map_err(|e| format!("{} is not a valid golden pair: {e}", path.display()))?;
        if pair.id.is_empty() {
            pair.id = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        pairs.push(pair);
    }
    Ok(pairs)
}

/// Count of each label in a golden set, for the calibrate summary line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LabelCounts {
    /// Number of `good` pairs.
    pub good: u32,
    /// Number of `bad` pairs.
    pub bad: u32,
    /// Number of `partial` pairs.
    pub partial: u32,
}

impl LabelCounts {
    /// Tally the labels across a loaded golden set.
    #[must_use]
    pub fn tally(pairs: &[GoldenPair]) -> Self {
        let mut c = Self::default();
        for p in pairs {
            match p.label {
                Label::Good => c.good += 1,
                Label::Bad => c.bad += 1,
                Label::Partial => c.partial += 1,
            }
        }
        c
    }

    /// Total number of pairs.
    #[must_use]
    pub const fn total(self) -> u32 {
        self.good + self.bad + self.partial
    }
}

/// A confusion matrix over the golden set.
///
/// "Positive" means "the judge flagged the pair as failing the gate".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Confusion {
    /// Flagged-as-bad and actually bad.
    pub true_positive: u32,
    /// Flagged-as-bad but actually good.
    pub false_positive: u32,
    /// Not-flagged and actually good.
    pub true_negative: u32,
    /// Not-flagged but actually bad.
    pub false_negative: u32,
}

impl Confusion {
    /// Record one (expected, judged-as-failing) outcome.
    pub fn record(&mut self, expected: Label, judged_failing: bool) {
        // Partial pairs are treated as "should not be hard-flagged".
        let actually_bad = expected == Label::Bad;
        match (actually_bad, judged_failing) {
            (true, true) => self.true_positive += 1,
            (true, false) => self.false_negative += 1,
            (false, true) => self.false_positive += 1,
            (false, false) => self.true_negative += 1,
        }
    }

    /// False-positive rate: `fp / (fp + tn)`. Returns `0.0` if denominator 0.
    ///
    /// `Confusion` is `Copy` (16 bytes), so it is taken by value.
    #[must_use]
    #[allow(clippy::float_arithmetic)] // a rate is intrinsically a division
    pub fn false_positive_rate(self) -> f64 {
        let denom = self.false_positive + self.true_negative;
        if denom == 0 {
            return 0.0;
        }
        f64::from(self.false_positive) / f64::from(denom)
    }

    /// False-negative rate: `fn / (fn + tp)`. Returns `0.0` if denominator 0.
    ///
    /// `Confusion` is `Copy` (16 bytes), so it is taken by value.
    #[must_use]
    #[allow(clippy::float_arithmetic)] // a rate is intrinsically a division
    pub fn false_negative_rate(self) -> f64 {
        let denom = self.false_negative + self.true_positive;
        if denom == 0 {
            return 0.0;
        }
        f64::from(self.false_negative) / f64::from(denom)
    }

    /// Whether the calibration gate passes: FPR `< 0.10` and FNR `< 0.20`.
    #[must_use]
    pub fn passes_gate(self) -> bool {
        self.false_positive_rate() < 0.10 && self.false_negative_rate() < 0.20
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_calibration_passes() {
        let mut c = Confusion::default();
        for _ in 0..5 {
            c.record(Label::Bad, true);
            c.record(Label::Good, false);
        }
        assert_eq!(c.false_positive_rate(), 0.0);
        assert_eq!(c.false_negative_rate(), 0.0);
        assert!(c.passes_gate());
    }

    #[test]
    fn too_many_false_positives_fails() {
        let mut c = Confusion::default();
        // 2 of 10 good pairs wrongly flagged → FPR 0.2 ≥ 0.10.
        for _ in 0..8 {
            c.record(Label::Good, false);
        }
        for _ in 0..2 {
            c.record(Label::Good, true);
        }
        assert!(c.false_positive_rate() > 0.10);
        assert!(!c.passes_gate());
    }

    #[test]
    fn label_parse_accepts_known_and_rejects_unknown() {
        assert_eq!(Label::parse("good").unwrap(), Label::Good);
        assert_eq!(Label::parse(" BAD ").unwrap(), Label::Bad);
        assert_eq!(Label::parse("Partial").unwrap(), Label::Partial);
        assert!(Label::parse("maybe").is_err());
    }

    #[test]
    fn load_golden_set_reads_sorted_and_tallies() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("b_bad.json"),
            r#"{"ac_text":"a","test_source":"b","label":"bad"}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("a_good.json"),
            r#"{"ac_text":"a","test_source":"b","label":"good"}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("c_partial.json"),
            r#"{"id":"explicit","ac_text":"a","test_source":"b","label":"partial"}"#,
        )
        .unwrap();
        // A non-json file must be ignored.
        std::fs::write(root.join("README.md"), "ignore me").unwrap();

        let pairs = load_golden_set(root).unwrap();
        assert_eq!(pairs.len(), 3);
        // Sorted by filename: a_good, b_bad, c_partial.
        assert_eq!(pairs[0].id, "a_good");
        assert_eq!(pairs[0].label, Label::Good);
        assert_eq!(pairs[1].id, "b_bad");
        // Explicit id in the file wins over the filename stem.
        assert_eq!(pairs[2].id, "explicit");

        let counts = LabelCounts::tally(&pairs);
        assert_eq!(counts.good, 1);
        assert_eq!(counts.bad, 1);
        assert_eq!(counts.partial, 1);
        assert_eq!(counts.total(), 3);
    }

    #[test]
    fn load_golden_set_rejects_bad_label() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("x.json"),
            r#"{"ac_text":"a","test_source":"b","label":"nonsense"}"#,
        )
        .unwrap();
        assert!(load_golden_set(dir.path()).is_err());
    }
}
