//! `claude-cli` backend: the Claude Code CLI in headless mode (`claude -p`).
//!
//! Last resort in `--backend auto` order: every call carries Claude Code's
//! own system prompt (~15-18k cached tokens) on top of the judge's, so it
//! costs more than the other two backends even though the OAuth login is
//! already on every fleet node.

use std::io::Read as _;
use std::process::{Command, Stdio};

use serde_json::Value;

use super::{Backend, Check, Error, which_env};

/// The environment variable overriding the `claude` binary path, for tests.
pub const BIN_ENV: &str = "AC_JUDGE_CLAUDE_BIN";

/// The `claude-cli` backend: one resolved binary path, one model shorthand.
pub struct ClaudeCliBackend {
    bin: std::path::PathBuf,
    model: String,
}

/// Build the `claude-cli` backend if the binary is resolvable on `$PATH`
/// (or `$AC_JUDGE_CLAUDE_BIN`).
///
/// # Errors
///
/// Returns a [`Check`] naming `claude` with outcome `"not on PATH"` when the
/// binary cannot be found.
pub fn build(model_override: Option<&str>) -> Result<ClaudeCliBackend, Check> {
    let bin = which_env(BIN_ENV, "claude").ok_or_else(|| Check {
        name: "claude",
        outcome: "not on PATH".to_owned(),
    })?;
    let model = model_override
        .unwrap_or(crate::DEFAULT_CLAUDE_CLI_MODEL)
        .to_owned();
    Ok(ClaudeCliBackend { bin, model })
}

impl Backend for ClaudeCliBackend {
    fn name(&self) -> &'static str {
        "claude-cli"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn judge(&self, system: &str, user: &str) -> Result<String, Error> {
        let mut child = Command::new(&self.bin)
            .arg("-p")
            .arg(user)
            .arg("--output-format")
            .arg("json")
            .arg("--model")
            .arg(&self.model)
            .arg("--tools")
            .arg("")
            .arg("--max-turns")
            .arg("1")
            .arg("--bare")
            .arg("--no-session-persistence")
            .arg("--append-system-prompt")
            .arg(system)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Transport(format!("spawn claude: {e}")))?;

        let status = super::wait_with_timeout(&mut child, super::call_timeout())?;

        let mut stdout = String::new();
        if let Some(mut so) = child.stdout.take() {
            let _ = so.read_to_string(&mut stdout);
        }
        if !status.success() {
            let mut stderr = String::new();
            if let Some(mut se) = child.stderr.take() {
                let _ = se.read_to_string(&mut stderr);
            }
            return Err(Error::Transport(format!(
                "claude -p exited {status}: {stderr}"
            )));
        }

        let envelope: Value = serde_json::from_str(stdout.trim())
            .map_err(|e| Error::BadResponse(format!("not a claude -p JSON envelope: {e}")))?;
        if envelope.get("is_error").and_then(Value::as_bool) == Some(true) {
            return Err(Error::BadResponse(format!(
                "claude -p reported is_error=true: {stdout}"
            )));
        }
        envelope
            .get("result")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| Error::BadResponse(format!("no .result field in envelope: {stdout}")))
    }
}
