//! Pluggable judge backends: `codex`, `api`, `claude-cli`.
//!
//! [`Backend`] is the trait every transport implements: send a system prompt
//! and a user prompt, get the model's raw reply text back. [`resolve`] picks
//! one according to `--backend auto|codex|api|claude-cli`, preferring Codex
//! first so the gate stays a cross-family check. An explicit `--backend`
//! that is unavailable never substitutes another backend — it errors.
//!
//! This module also owns the parts that are the same regardless of which
//! backend answered: the on-disk verdict cache (keyed by backend + model, so
//! two backends judging the same AC/test pair never collide), and parsing
//! the strict-JSON verdict out of a backend's reply text.

pub mod api;
pub mod claude_cli;
pub mod codex;

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// Errors a backend's `judge` call (or its own availability check) can
/// return.
#[derive(Debug)]
pub enum Error {
    /// The HTTP or subprocess transport failed (including a timeout).
    Transport(String),
    /// The reply could not be parsed as a verdict, or (for `claude-cli`) the
    /// envelope carried `is_error: true`.
    BadResponse(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(m) => write!(f, "transport error: {m}"),
            Self::BadResponse(m) => write!(f, "bad response: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// A judge backend: sends the system + user text and returns the model's
/// raw reply text (expected to be the strict verdict JSON).
pub trait Backend {
    /// Backend name, as recorded in the receipt (`"codex" | "api" | "claude-cli"`).
    fn name(&self) -> &'static str;
    /// The model identifier this backend is using.
    fn model(&self) -> &str;
    /// Send `system` + `user` and return the model's reply text.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] on a subprocess/HTTP failure (including
    /// hitting the per-call deadline), and [`Error::BadResponse`] when the
    /// reply cannot be extracted from the backend's envelope.
    fn judge(&self, system: &str, user: &str) -> Result<String, Error>;
}

/// Which backend the operator asked for via `--backend`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requested {
    /// Try codex, then api, then claude-cli; use the first available.
    Auto,
    /// Only codex; error (never substitute) if unavailable.
    Codex,
    /// Only the Anthropic Messages API; error if unavailable.
    Api,
    /// Only the Claude Code CLI in headless mode; error if unavailable.
    ClaudeCli,
}

impl Requested {
    /// Parse the `--backend` flag value.
    ///
    /// # Errors
    ///
    /// Returns `Err` with a description of the invalid token when `s` is not
    /// one of `auto | codex | api | claude-cli`.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "auto" => Ok(Self::Auto),
            "codex" => Ok(Self::Codex),
            "api" => Ok(Self::Api),
            "claude-cli" => Ok(Self::ClaudeCli),
            other => Err(format!(
                "unknown backend {other:?} (expected auto|codex|api|claude-cli)"
            )),
        }
    }
}

/// One backend's availability check, for the exit-6 diagnostic.
#[derive(Debug, Clone)]
pub struct Check {
    /// The backend this check is about (`"codex" | "api" | "claude-cli"`).
    pub name: &'static str,
    /// Human-readable reason it is unavailable (e.g. `"not on PATH"`).
    pub outcome: String,
}

impl fmt::Display for Check {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.outcome)
    }
}

/// Resolve a [`Backend`] per `requested`, given an optional `--model`
/// override (applies to whichever backend is selected; when absent each
/// backend uses its own pinned default).
///
/// # Errors
///
/// Returns the list of failed availability checks when no backend can be
/// used. For an explicit (non-`auto`) request this is always exactly one
/// check; `auto` returns all three so the caller can print the full
/// diagnostic. An explicit `--backend` that is unavailable never falls back
/// to another backend.
pub fn resolve(
    requested: Requested,
    model_override: Option<&str>,
) -> Result<Box<dyn Backend>, Vec<Check>> {
    match requested {
        Requested::Codex => codex::build(model_override).map(boxed).map_err(|c| vec![c]),
        Requested::Api => api::build(model_override).map(boxed).map_err(|c| vec![c]),
        Requested::ClaudeCli => claude_cli::build(model_override)
            .map(boxed)
            .map_err(|c| vec![c]),
        Requested::Auto => {
            let mut checks = Vec::with_capacity(3);
            match codex::build(model_override) {
                Ok(b) => return Ok(boxed(b)),
                Err(c) => checks.push(c),
            }
            match api::build(model_override) {
                Ok(b) => return Ok(boxed(b)),
                Err(c) => checks.push(c),
            }
            match claude_cli::build(model_override) {
                Ok(b) => return Ok(boxed(b)),
                Err(c) => checks.push(c),
            }
            Err(checks)
        }
    }
}

/// Box a concrete backend as a trait object via return-type unsizing —
/// avoids the `as` cast `clippy::as_conversions` (denied crate-wide) would
/// otherwise flag on `Box::new(b) as Box<dyn Backend>`.
fn boxed<B: Backend + 'static>(b: B) -> Box<dyn Backend> {
    Box::new(b)
}

/// Per-call subprocess deadline. Overridable by `AC_JUDGE_CALL_TIMEOUT_SECS`
/// for tests; defaults to 120s per the PRD.
#[must_use]
pub fn call_timeout() -> Duration {
    std::env::var("AC_JUDGE_CALL_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map_or(Duration::from_secs(120), Duration::from_secs)
}

/// Resolve a subprocess backend's binary: `$env_var` if set, else `default`
/// resolved against `$PATH`. Returns `None` if neither exists.
pub(crate) fn which_env(env_var: &str, default: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var(env_var) {
        let path = PathBuf::from(p);
        return path.is_file().then_some(path);
    }
    which(default)
}

/// A minimal `$PATH` search, so subprocess backends don't need a `which`
/// crate dependency.
fn which(cmd: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var).find_map(|dir| {
        let candidate = dir.join(cmd);
        candidate.is_file().then_some(candidate)
    })
}

/// Poll `child` until it exits or `timeout` elapses. On timeout, kills the
/// child and reaps it before returning [`Error::Transport`] with message
/// `"timeout"` (so `Display` renders `"transport error: timeout"`, per AC9).
pub(crate) fn wait_with_timeout(
    child: &mut Child,
    timeout: Duration,
) -> Result<std::process::ExitStatus, Error> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(e) => return Err(Error::Transport(e.to_string())),
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Transport("timeout".to_owned()));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Compute the verdict cache key:
/// `sha256(ac_text + test_source + backend + model + prompt_version)`.
///
/// Backend is part of the key (req. 9 / AC11) so two backends judging the
/// same AC/test pair never collide, and the `PROMPT_VERSION` bump on this
/// PRD orphans v0.1 cache entries rather than reusing them.
#[must_use]
pub fn cache_key(ac_text: &str, test_source: &str, backend: &str, model: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ac_text.as_bytes());
    hasher.update(test_source.as_bytes());
    hasher.update(backend.as_bytes());
    hasher.update(model.as_bytes());
    hasher.update(crate::PROMPT_VERSION.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Send one AC + test pair through `backend`, checking the on-disk cache first.
///
/// On a cache miss, calls `backend.judge()`, parses the strict-JSON reply
/// into a [`crate::schema::Verdict`], and writes the result back to cache
/// (best-effort; a cache-write failure is not fatal).
///
/// # Errors
///
/// Returns an error if the backend call fails, or its reply cannot be
/// parsed into a verdict.
#[allow(clippy::too_many_arguments)]
pub fn judge_one_ac(
    backend: &dyn Backend,
    ac_id: &str,
    ac_text: &str,
    test_path: Option<&str>,
    test_source: &str,
    crate_name: &str,
    ac_index: u32,
    cache_dir: &Path,
) -> Result<crate::schema::Verdict, Error> {
    let key = cache_key(ac_text, test_source, backend.name(), backend.model());
    let cache_file = cache_dir.join(format!("{key}.json"));

    // Cache hit: return stored verdict, calling the backend zero times.
    if let Ok(cached) = fs::read_to_string(&cache_file) {
        if let Ok(v) = serde_json::from_str::<crate::schema::Verdict>(&cached) {
            return Ok(v);
        }
    }

    // Cache miss: call the backend.
    let user_content =
        crate::prompt::build_user_content(ac_text, test_source, crate_name, ac_index);
    let reply = backend.judge(crate::prompt::SYSTEM_TEXT, &user_content)?;
    let verdict = parse_verdict(ac_id, test_path.map(str::to_owned), &reply)?;

    // Write to cache (best-effort; failures are non-fatal).
    if let Ok(json) = serde_json::to_string_pretty(&verdict) {
        let _ = fs::create_dir_all(cache_dir);
        let _ = fs::write(&cache_file, json);
    }

    Ok(verdict)
}

/// Parse a backend's strict-JSON reply text into a [`crate::schema::Verdict`]
/// for `ac_id`.
///
/// Tolerates the verdict being wrapped in a fenced code block or surrounded
/// by prose, so a backend that talks a little before or after the JSON does
/// not fail the run.
///
/// # Errors
///
/// Returns [`Error::BadResponse`] if no JSON object with the expected fields
/// can be extracted.
pub fn parse_verdict(
    ac_id: &str,
    test_path: Option<String>,
    reply_text: &str,
) -> Result<crate::schema::Verdict, Error> {
    #[derive(serde::Deserialize)]
    struct Raw {
        behavior_match: crate::schema::BehaviorMatch,
        assertion_kind: crate::schema::AssertionKind,
        confidence: f64,
        reasoning: String,
    }
    let json_slice = extract_json_object(reply_text)
        .ok_or_else(|| Error::BadResponse("no JSON object in reply".to_owned()))?;
    let raw: Raw =
        serde_json::from_str(json_slice).map_err(|e| Error::BadResponse(e.to_string()))?;
    Ok(crate::schema::Verdict {
        ac_id: ac_id.to_owned(),
        test_path,
        // The backend never knows an AC's level — it judges test source
        // against AC text only. The caller (`main::build_verdicts`) fills
        // this in from the parsed `Ac` after the call returns.
        level: None,
        behavior_match: raw.behavior_match,
        assertion_kind: raw.assertion_kind,
        confidence: raw.confidence,
        reasoning: raw.reasoning,
    })
}

/// Extract the first balanced `{...}` JSON object from arbitrary text.
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    for (offset, ch) in text[start..].char_indices() {
        if in_str {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return text.get(start..=start + offset);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{AssertionKind, BehaviorMatch};

    #[test]
    fn cache_key_is_stable_and_backend_sensitive() {
        let a = cache_key("ac", "test", "codex", "m");
        let b = cache_key("ac", "test", "codex", "m");
        let c = cache_key("ac", "test", "api", "m");
        assert_eq!(a, b);
        assert_ne!(a, c, "backend must be part of the cache key");
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn parses_verdict_with_surrounding_prose() {
        let reply = "Here is my verdict:\n```json\n{\"behavior_match\": \"yes\", \
\"assertion_kind\": \"asserts-invariant\", \"confidence\": 0.9, \"reasoning\": \"ok\"}\n```";
        let v = parse_verdict("AC1", Some("tests/ac1_x.rs".to_owned()), reply).unwrap();
        assert_eq!(v.behavior_match, BehaviorMatch::Yes);
        assert_eq!(v.assertion_kind, AssertionKind::AssertsInvariant);
        assert!((v.confidence - 0.9).abs() < 1e-9);
    }

    #[test]
    fn requested_parse_rejects_unknown() {
        assert!(Requested::parse("bogus").is_err());
        assert_eq!(Requested::parse("auto").unwrap(), Requested::Auto);
    }
}
