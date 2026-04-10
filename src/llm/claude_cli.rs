//! `claude` CLI subprocess wrapper.
//!
//! Spawns `claude -p --output-format json --json-schema <schema>` with
//! the prompt as trailing argument, parses the JSON response from stdout.
//! Uses the local Claude subscription (no API key).

use anyhow::{Context, Result};
use std::process::Command;
use std::sync::OnceLock;

/// True if the `claude` CLI is installed and responds to `--version`.
pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("claude")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// Configuration for a single LLM invocation.
#[derive(Debug)]
pub struct LlmRequest<'a> {
    pub prompt: &'a str,
    pub json_schema: &'a str,
    pub model: &'a str,
}

/// Invoke `claude -p` and return the parsed JSON response string.
///
/// The caller is responsible for deserializing the JSON into their
/// rule-specific response type.
pub fn invoke(req: &LlmRequest) -> Result<String> {
    let mut child = Command::new("claude")
        .args([
            "-p",
            "--output-format", "json",
            "--json-schema", req.json_schema,
            "--no-session-persistence",
            "--model", req.model,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to spawn `claude` CLI — is it installed?")?;

    // Write prompt to stdin, then drop to signal EOF.
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(req.prompt.as_bytes());
    }

    let output = child.wait_with_output().context("claude subprocess failed")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "claude CLI exited with {}: {}",
            output.status,
            stderr.lines().next().unwrap_or("(no stderr)")
        );
    }

    let stdout = String::from_utf8(output.stdout)
        .context("claude output is not valid UTF-8")?;

    if stdout.trim().is_empty() {
        anyhow::bail!("claude returned empty output");
    }

    // claude --output-format json wraps the response in a JSON envelope.
    // The structured output (from --json-schema) is in the "result" field.
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .with_context(|| {
            format!(
                "failed to parse claude JSON: {}",
                &stdout[..stdout.len().min(200)]
            )
        })?;

    if let Some(result) = parsed.get("result").and_then(|v| v.as_str()) {
        Ok(result.to_string())
    } else if let Some(result) = parsed.get("result") {
        Ok(result.to_string())
    } else {
        // Fallback: the entire output IS the result.
        Ok(stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_available_returns_bool() {
        let _ = is_available();
    }
}
