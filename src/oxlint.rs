//! oxlint subprocess — runs oxlint on TS/JS files and converts JSON output
//! into unified Diagnostic structs.
//!
//! How it works:
//! 1. Check if oxlint binary is on PATH — error with install instructions if not.
//! 2. Invoke `oxlint --format json` with file paths as args.
//! 3. Parse the JSON envelope (`diagnostics` array) and map each entry
//!    to our Diagnostic model using `labels[0].span` for line/column.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;

/// Top-level oxlint JSON output envelope.
#[derive(Deserialize)]
struct OxlintOutput {
    #[serde(default)]
    diagnostics: Vec<OxlintDiag>,
}

/// A single oxlint diagnostic — adapted from actual oxlint 1.59 JSON format.
#[derive(Deserialize)]
struct OxlintDiag {
    #[serde(default)]
    message: String,
    /// Rule identifier, e.g. "eslint(no-unused-vars)".
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    filename: String,
    /// Position labels — first label carries the primary span.
    #[serde(default)]
    labels: Vec<OxlintLabel>,
}

#[derive(Deserialize)]
struct OxlintLabel {
    #[serde(default)]
    span: OxlintSpan,
}

#[derive(Deserialize, Default)]
struct OxlintSpan {
    #[serde(default)]
    line: usize,
    #[serde(default)]
    column: usize,
}

/// Check if oxlint binary is on PATH.
pub fn is_available() -> bool {
    Command::new("oxlint")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Run oxlint on the given TS/JS files and return unified diagnostics.
#[must_use = "diagnostics should be collected into the final report"]
pub fn run(files: &[&SourceFile], config_path: Option<&Path>) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }

    let mut cmd = Command::new("oxlint");
    cmd.arg("--format").arg("json");
    if let Some(cfg) = config_path {
        cmd.arg("-c").arg(cfg);
    }
    for f in files {
        cmd.arg(&f.path);
    }

    let output = cmd.output().context(
        "failed to run oxlint — install it with: npm install -g oxlint",
    )?;

    // oxlint exits 1 when violations found — that's normal, not an error.
    if !output.status.success() && output.status.code() != Some(1) {
        bail!(
            "oxlint crashed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_json(&stdout)
}

/// Parse oxlint JSON output into unified Diagnostic structs.
fn parse_json(json: &str) -> Result<Vec<Diagnostic>> {
    let envelope: OxlintOutput =
        serde_json::from_str(json).context("failed to parse oxlint JSON output")?;

    Ok(envelope
        .diagnostics
        .into_iter()
        .map(|d| {
            let (line, column) = d
                .labels
                .first()
                .map(|l| (l.span.line, l.span.column))
                .unwrap_or((0, 0));

            Diagnostic {
                path: d.filename.into(),
                line,
                column,
                rule_id: d.code.unwrap_or_else(|| "oxlint/unknown".into()),
                message: d.message,
                severity: if d.severity == "warning" {
                    Severity::Warning
                } else {
                    Severity::Error
                },
            }
        })
        .collect())
}
