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
//! The judge itself runs on a pluggable backend, resolved in this order
//! (`--backend auto`, the default): **codex** (`codex exec`, a different
//! model family from the Claude implementer — the independence the design
//! calls for), then the **api** backend (`$ANTHROPIC_API_KEY` against the
//! Anthropic Messages API), then **claude-cli** (`claude -p` in headless
//! mode, authenticated by the operator's existing Claude login). An
//! explicit `--backend` never substitutes another backend if the requested
//! one is unavailable; it fails loudly instead. `exit::NO_BACKEND` (6) means
//! none of the three checked out, and the diagnostic on stderr says which
//! three things were checked and why each failed. See [`backend`] for the
//! trait and resolution logic.
//!
//! This crate is split into small modules so each surface can be tested in
//! isolation:
//!
//! - [`pair`] — AC ↔ test pair detection (no network).
//! - [`prompt`] — system text + per-AC user content shared by every backend.
//! - [`backend`] — the `Backend` trait, `codex`/`api`/`claude-cli`
//!   transports, resolution order, the verdict cache, and reply parsing.
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

pub mod backend;
pub mod calibrate;
pub mod pair;
pub mod prompt;
pub mod schema;

/// Version stamped into every verdict so the SHA cache key changes when the prompt or schema changes.
///
/// Bumped for this PRD: the cache key now also includes the backend name, so
/// v0.1 entries are orphaned regardless.
pub const PROMPT_VERSION: &str = "v0.2";

/// The default judge model for the `api` backend.
///
/// Intentionally a different family from the autobuilder pipeline default
/// (Opus): the model that wrote the test should not also judge whether the
/// test verifies its AC.
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

/// The default judge model for the `codex` backend.
///
/// The installed CLI's own default, verified live on `RedBaron` 2026-09-03.
/// Pinned rather than left to `codex exec`'s own default so the receipt's
/// `model` field is always accurate even if that default later changes
/// underfoot.
pub const DEFAULT_CODEX_MODEL: &str = "gpt-5.6-sol";

/// The default judge model for the `claude-cli` backend: the CLI's short
/// alias for [`DEFAULT_MODEL`] (`claude -p --model sonnet`, not the full
/// `claude-sonnet-4-6` id `api` uses).
pub const DEFAULT_CLAUDE_CLI_MODEL: &str = "sonnet";

/// Exit codes, per the PRD's contract.
pub mod exit {
    /// All ACs passed the judge.
    pub const PASS: u8 = 0;
    /// At least one AC failed the judge gate.
    pub const AC_FAIL: u8 = 4;
    /// No judge backend is available.
    ///
    /// `codex login`, `$ANTHROPIC_API_KEY`, and `claude login` were all
    /// checked and none worked (or the single explicitly
    /// `--backend`-requested one didn't). No network call is attempted
    /// before this exit. Value unchanged from the v0.1 `NO_API_KEY` code
    /// this replaces; only its meaning widened.
    pub const NO_BACKEND: u8 = 6;
}
