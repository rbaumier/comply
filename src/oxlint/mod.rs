//! oxlint subprocess — invokes oxlint on TS/JS files and converts JSON output
//! into unified Diagnostic structs.
//!
//! How it works:
//! 1. `is_available()` checks the binary is on PATH. Result is cached in a
//!    `OnceLock` so we don't fork oxlint on every invocation.
//! 2. `lint_files()` collects every `Backend::Oxlint` binding from the rule
//!    registry, passes them to `oxlint_config::generate` to produce the
//!    runtime config, then invokes `oxlint --format json -c <config>` with
//!    file paths terminated by `--` (so paths starting with `-` don't look
//!    like flags).
//! 3. Parses the JSON envelope from raw bytes and remaps each diagnostic's
//!    rule-id + severity through the comply registry so users see
//!    `[no-explicit-any]` instead of `typescript-eslint(no-explicit-any)`.

mod options;
mod remap;
mod schema;

pub use options::for_rule as options_for;

use anyhow::{Context, Result, bail};
use rustc_hash::FxHashMap;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;
use crate::rules::meta::RuleMeta;
use schema::{OxlintDiag, OxlintOutput, OxlintSeverity};

/// Max files per oxlint invocation. Conservative chunk size to avoid ARG_MAX.
const FILES_PER_BATCH: usize = 500;

/// Check if oxlint binary is on PATH. Result is cached for the process lifetime.
pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("oxlint")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// Invoke oxlint on the given TS/JS files and return unified diagnostics.
/// Always enables type-aware rules via `--type-aware` (requires oxlint-tsgolint).
#[must_use = "diagnostics from oxlint must be reported"]
pub fn lint_files(
    files: &[&SourceFile],
    config: &crate::config::Config,
) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }
    let mut bindings = crate::rules::collect_oxlint_bindings();
    bindings.extend(crate::rules::collect_tsgolint_bindings());
    if bindings.is_empty() {
        return Ok(vec![]);
    }
    let rule_entries: Vec<crate::oxlint_config::RuleEntry> = bindings
        .iter()
        .map(|(key, _, sev)| (*key, *sev, options::for_rule(key, config)))
        .collect();
    let oxlint_config = crate::oxlint_config::generate(&rule_entries)?;
    let remap = remap::build_table(&bindings);

    let mut all = Vec::with_capacity(files.len());
    for batch in files.chunks(FILES_PER_BATCH) {
        let output = invoke_oxlint(batch, Some(oxlint_config.path()))?;
        all.extend(parse_json_bytes(&output.stdout, &output.stderr, &remap)?);
    }
    Ok(all)
}

/// Spawn oxlint as a subprocess and validate exit status.
fn invoke_oxlint(
    files: &[&SourceFile],
    config_path: Option<&Path>,
) -> Result<std::process::Output> {
    let mut cmd = Command::new("oxlint");
    cmd.args(["--format", "json", "--type-aware"]);
    if let Some(cfg) = config_path {
        cmd.arg("-c").arg(cfg);
    }
    // `--` terminates option parsing so file paths starting with `-` are not
    // interpreted as flags by oxlint.
    cmd.arg("--");
    for f in files {
        cmd.arg(&f.path);
    }

    let output = cmd
        .output()
        .context("failed to invoke oxlint — install it with: npm install -g oxlint")?;

    // oxlint exits 1 when violations are found — that is normal, not an error.
    if !output.status.success() && output.status.code() != Some(1) {
        bail!(
            "oxlint crashed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output)
}

/// Parse oxlint JSON output bytes into unified Diagnostic structs.
fn parse_json_bytes(
    stdout: &[u8],
    stderr: &[u8],
    remap: &FxHashMap<String, &'static RuleMeta>,
) -> Result<Vec<Diagnostic>> {
    let envelope: OxlintOutput = serde_json::from_slice(stdout).with_context(|| {
        format!(
            "failed to parse oxlint JSON output. oxlint stderr: {}",
            String::from_utf8_lossy(stderr)
        )
    })?;
    Ok(envelope
        .diagnostics
        .into_iter()
        .map(|d| into_diagnostic(d, remap))
        .collect())
}

/// Convert one oxlint diagnostic into our unified format, remapping the
/// rule_id + severity through the registry when a match exists.
fn into_diagnostic(d: OxlintDiag, remap: &FxHashMap<String, &'static RuleMeta>) -> Diagnostic {
    let (line, column) = d
        .labels
        .first()
        .map(|l| (l.span.line.max(1), l.span.column.max(1)))
        .unwrap_or((1, 1));

    let oxlint_code = d.code.clone().unwrap_or_default();
    let (rule_id, severity) = match remap.get(&oxlint_code) {
        Some(meta) => (std::borrow::Cow::Borrowed(meta.id), meta.severity),
        None => (
            std::borrow::Cow::Owned(d.code.unwrap_or_else(|| "oxlint/unknown".into())),
            match d.severity {
                OxlintSeverity::Warning | OxlintSeverity::Advice => Severity::Warning,
                OxlintSeverity::Error => Severity::Error,
            },
        ),
    };

    Diagnostic {
        path: std::sync::Arc::from(std::path::PathBuf::from(d.filename).as_path()),
        line,
        column,
        rule_id,
        message: d.message,
        severity,
        span: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_position_is_one_one_not_zero_zero() {
        let remap = FxHashMap::default();
        let json = br#"{ "diagnostics": [{"message": "X", "severity": "error", "filename": "/tmp/x.ts", "labels": []}] }"#;
        let result = parse_json_bytes(json, b"", &remap).expect("must parse");
        assert_eq!(result[0].line, 1);
        assert_eq!(result[0].column, 1);
    }

    #[test]
    fn empty_diagnostics_array_yields_empty_vec() {
        let remap = FxHashMap::default();
        let json = br#"{ "diagnostics": [] }"#;
        let result = parse_json_bytes(json, b"", &remap).expect("must parse");
        assert!(result.is_empty());
    }
}
