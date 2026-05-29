//! ac-judge — a semantic acceptance-criteria judge.
//!
//! For each declared acceptance criterion (AC) in a PRD, `ac-judge` pairs
//! the AC's English text with the test file that claims to verify it, sends
//! both to an independent model, and asks two strict questions:
//!
//! 1. does the test exercise the behavior the AC describes
//!    (`behavior_match`)?
//! 2. is the test asserting the AC's stated invariant, or merely re-running
//!    the implementation and confirming its return (`assertion_kind`)?
//!
//! The verdicts land in a 9th autobuilder receipt at
//! `target/autobuilder/ac-semantic-judge.json`. The `run` subcommand exits
//! non-zero if any AC fails the gate, so the autobuilder pipeline can block
//! on it.
//!
//! This crate is split into small modules so each surface can be tested in
//! isolation:
//!
//! - [`pair`] — AC ↔ test pair detection (no network).
//! - [`prompt`] — system + few-shot Anthropic request assembly.
//! - [`api`] — sync Anthropic client (`ureq`).
//! - [`schema`] — verdict + receipt JSON types and exit-code contract.
//! - [`calibrate`] — golden-set runner.

#![cfg_attr(not(test), forbid(unsafe_code))]
// Unit-test bodies legitimately use unwrap/indexing/exact-float asserts and
// arithmetic in fixtures; the BAD_RUST restriction lints target shipped code,
// not the tests that prove it. Scope the relaxation to `cfg(test)` only.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::float_cmp,
        clippy::float_arithmetic,
        clippy::panic
    )
)]

pub mod api;
pub mod calibrate;
pub mod pair;
pub mod prompt;
pub mod schema;

/// Version stamped into every verdict so the SHA cache key changes when the
/// prompt or schema changes. Bump on any prompt/schema change.
pub const PROMPT_VERSION: &str = "v0.1";

/// The default judge model. Intentionally a different family from the
/// autobuilder pipeline default (Opus): the model that wrote the test should
/// not also judge whether the test verifies its AC.
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

/// Exit codes, per the PRD's contract.
pub mod exit {
    /// All ACs passed the judge.
    pub const PASS: u8 = 0;
    /// At least one AC failed the judge gate.
    pub const AC_FAIL: u8 = 4;
    /// `$ANTHROPIC_API_KEY` is unset; no network call was attempted.
    pub const NO_API_KEY: u8 = 6;
}
