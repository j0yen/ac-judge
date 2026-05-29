//! Verdict and receipt JSON types.
//!
//! These mirror the JSON Schema shipped at
//! `schemas/ac-semantic-judge.schema.json`. The receipt is the 9th
//! autobuilder receipt written to `target/autobuilder/ac-semantic-judge.json`.

use serde::{Deserialize, Serialize};

/// Whether the test exercises the behavior the AC's English describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BehaviorMatch {
    /// The test exercises the AC's behavior.
    Yes,
    /// The test does not exercise the AC's behavior (or no test exists).
    No,
    /// The test exercises some, but not all, of the AC's behavior.
    Partial,
}

/// What kind of assertion the test makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssertionKind {
    /// Asserts a property the AC's English describes.
    AssertsInvariant,
    /// Calls the function and asserts it returned what it returned (tautology).
    RestatesImpl,
    /// A mix of invariant and tautological assertions.
    Mixed,
    /// No paired test, so no assertion to classify.
    None,
}

/// The judge's verdict for a single AC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    /// The AC identifier, e.g. `AC1`.
    pub ac_id: String,
    /// Path to the paired test file, relative to the crate root, or `null`
    /// when the AC is unpaired.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_path: Option<String>,
    /// Whether the test exercises the AC's behavior.
    pub behavior_match: BehaviorMatch,
    /// What kind of assertion the test makes.
    pub assertion_kind: AssertionKind,
    /// Confidence in the verdict, `0.0..=1.0`.
    pub confidence: f64,
    /// One-to-two-sentence rationale, or the unpaired reason.
    pub reasoning: String,
}

impl Verdict {
    /// Construct the canonical "no paired test found" verdict.
    #[must_use]
    pub fn unpaired(ac_id: impl Into<String>) -> Self {
        Self {
            ac_id: ac_id.into(),
            test_path: None,
            behavior_match: BehaviorMatch::No,
            assertion_kind: AssertionKind::None,
            confidence: 1.0,
            reasoning: "no paired test found".to_owned(),
        }
    }

    /// Whether this verdict fails the Stage-4 gate.
    ///
    /// A verdict fails if `behavior_match == No`, OR if it both
    /// `restates-impl` and carries confidence `>= 0.7`.
    #[must_use]
    pub fn fails_gate(&self) -> bool {
        if self.behavior_match == BehaviorMatch::No {
            return true;
        }
        self.assertion_kind == AssertionKind::RestatesImpl && self.confidence >= 0.7
    }
}

/// The full receipt written to `target/autobuilder/ac-semantic-judge.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    /// Schema discriminator.
    pub schema: String,
    /// Path to the PRD that declared the ACs.
    pub prd_source: String,
    /// Path to the crate root that was judged.
    pub crate_root: String,
    /// The model that produced the verdicts.
    pub model: String,
    /// The prompt/schema version stamped into the cache key.
    pub prompt_version: String,
    /// ISO-8601 timestamp of the run.
    pub generated_at: String,
    /// Whether every verdict passed the gate.
    pub passed: bool,
    /// One verdict per declared AC.
    pub verdicts: Vec<Verdict>,
}

/// The schema discriminator value for receipts.
pub const RECEIPT_SCHEMA: &str = "autobuilder.ac_semantic_judge.v1";

impl Receipt {
    /// Assemble a receipt from its verdicts, computing `passed`.
    #[must_use]
    pub fn new(
        prd_source: impl Into<String>,
        crate_root: impl Into<String>,
        model: impl Into<String>,
        generated_at: impl Into<String>,
        verdicts: Vec<Verdict>,
    ) -> Self {
        let passed = verdicts.iter().all(|v| !v.fails_gate());
        Self {
            schema: RECEIPT_SCHEMA.to_owned(),
            prd_source: prd_source.into(),
            crate_root: crate_root.into(),
            model: model.into(),
            prompt_version: crate::PROMPT_VERSION.to_owned(),
            generated_at: generated_at.into(),
            passed,
            verdicts,
        }
    }
}
