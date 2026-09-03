//! `codex` backend: `codex exec` in headless, sandboxed mode.
//!
//! Codex is preferred by `--backend auto` (tried first) because it is a
//! genuinely different model family from the Claude implementer that wrote
//! the test being judged — the whole point of an independent judge.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

use super::{Backend, Check, Error, which_env};

/// The environment variable overriding the `codex` binary path, for tests.
pub const BIN_ENV: &str = "AC_JUDGE_CODEX_BIN";

/// The strict JSON Schema handed to `codex exec --output-schema`, matching
/// the verdict fields the judge prompt asks for (not the full receipt
/// verdict, which also carries `ac_id`/`test_path` that codex never sees).
const VERDICT_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "required": ["behavior_match", "assertion_kind", "confidence", "reasoning"],
  "properties": {
    "behavior_match": { "enum": ["yes", "no", "partial"] },
    "assertion_kind": { "enum": ["asserts-invariant", "restates-impl", "mixed", "none"] },
    "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
    "reasoning": { "type": "string" }
  }
}"#;

/// The `codex` backend: one resolved binary path, one model.
pub struct CodexBackend {
    bin: std::path::PathBuf,
    model: String,
}

/// Build the `codex` backend if the binary is resolvable and
/// `codex login status` exits 0.
///
/// # Errors
///
/// Returns a [`Check`] naming `codex` with outcome `"not on PATH"` when the
/// binary cannot be found, or a login-failure outcome when the binary runs
/// but reports it is not logged in. Neither path makes a network call.
pub fn build(model_override: Option<&str>) -> Result<CodexBackend, Check> {
    let bin = which_env(BIN_ENV, "codex").ok_or_else(|| Check {
        name: "codex",
        outcome: "not on PATH".to_owned(),
    })?;

    let status = Command::new(&bin)
        .arg("login")
        .arg("status")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            return Err(Check {
                name: "codex",
                outcome: format!("not logged in (`codex login status` exited {s})"),
            });
        }
        Err(e) => {
            return Err(Check {
                name: "codex",
                outcome: format!("cannot run `codex login status`: {e}"),
            });
        }
    }

    let model = model_override
        .unwrap_or(crate::DEFAULT_CODEX_MODEL)
        .to_owned();
    Ok(CodexBackend { bin, model })
}

impl Backend for CodexBackend {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn judge(&self, system: &str, user: &str) -> Result<String, Error> {
        let tmp = tempfile::tempdir().map_err(|e| Error::Transport(format!("tempdir: {e}")))?;
        let schema_path = tmp.path().join("verdict.schema.json");
        fs::write(&schema_path, VERDICT_SCHEMA)
            .map_err(|e| Error::Transport(format!("write schema: {e}")))?;
        let out_path = tmp.path().join("out.txt");

        let mut child = Command::new(&self.bin)
            .arg("exec")
            .arg("--sandbox")
            .arg("read-only")
            .arg("--skip-git-repo-check")
            .arg("--ephemeral")
            .arg("--ignore-user-config")
            .arg("--ignore-rules")
            .arg("-C")
            .arg(tmp.path())
            .arg("--model")
            .arg(&self.model)
            .arg("--output-schema")
            .arg(&schema_path)
            .arg("--output-last-message")
            .arg(&out_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Transport(format!("spawn codex: {e}")))?;

        // Prompt on stdin: system text, blank line, user content.
        if let Some(mut stdin) = child.stdin.take() {
            let _ = write!(stdin, "{system}\n\n{user}");
            // Drop closes stdin so `codex exec` sees EOF.
        }

        let status = super::wait_with_timeout(&mut child, super::call_timeout())?;
        if !status.success() {
            let mut stderr = String::new();
            if let Some(mut se) = child.stderr.take() {
                use std::io::Read as _;
                let _ = se.read_to_string(&mut stderr);
            }
            return Err(Error::Transport(format!(
                "codex exec exited {status}: {stderr}"
            )));
        }

        fs::read_to_string(&out_path)
            .map_err(|e| Error::BadResponse(format!("cannot read --output-last-message: {e}")))
    }
}
