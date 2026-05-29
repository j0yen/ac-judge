//! Golden-set calibration runner.
//!
//! `ac-judge calibrate --golden-set <dir>` runs the judge against
//! hand-curated AC-↔-test pairs labeled good/bad/partial and reports the
//! confusion matrix. The CI gate is false-positive `< 0.10` and
//! false-negative `< 0.20`.
//!
//! The judge call against each golden pair is network-bound and deferred to a
//! later iteration; the confusion-matrix math below is pure and tested now.

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
}
