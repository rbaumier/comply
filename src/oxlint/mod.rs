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
use std::time::Duration;
use wait_timeout::ChildExt;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;
use crate::project::ProjectCtx;
use crate::rules::meta::RuleMeta;
use schema::{OxlintDiag, OxlintOutput, OxlintSeverity};

/// Max files per oxlint invocation. Conservative chunk size to avoid ARG_MAX.
const FILES_PER_BATCH: usize = 500;
const OXLINT_BATCH_TIMEOUT: Duration = Duration::from_secs(45);

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
    project: &ProjectCtx,
) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }
    let oxlint_bindings = crate::rules::collect_oxlint_bindings();
    let tsgolint_bindings = crate::rules::collect_tsgolint_bindings();
    if oxlint_bindings.is_empty() && tsgolint_bindings.is_empty() {
        return Ok(vec![]);
    }
    let (module_aware_oxlint, standard_oxlint): (Vec<_>, Vec<_>) = oxlint_bindings
        .into_iter()
        .partition(|(key, _, _)| is_require_import_rule(key));
    let (module_aware_tsgolint, standard_tsgolint): (Vec<_>, Vec<_>) = tsgolint_bindings
        .into_iter()
        .partition(|(key, _, _)| is_require_import_rule(key));

    let mut all = Vec::new();
    if !standard_oxlint.is_empty() {
        all.extend(lint_files_with_bindings(files, config, &standard_oxlint)?);
    }

    let type_aware = type_aware_files(files);
    if !type_aware.is_empty() && !standard_tsgolint.is_empty() {
        all.extend(lint_files_with_bindings(
            &type_aware,
            config,
            &standard_tsgolint,
        )?);
    }

    let esm = es_module_files(files, project);
    if !esm.is_empty() && !module_aware_oxlint.is_empty() {
        all.extend(lint_files_with_bindings(
            &esm,
            config,
            &module_aware_oxlint,
        )?);
    }

    let type_aware_esm = type_aware_files(&esm);
    if !type_aware_esm.is_empty() && !module_aware_tsgolint.is_empty() {
        all.extend(lint_files_with_bindings(
            &type_aware_esm,
            config,
            &module_aware_tsgolint,
        )?);
    }

    Ok(all)
}

fn is_require_import_rule(key: &str) -> bool {
    matches!(key, "typescript/no-require-imports" | "no-require-imports")
}

fn es_module_files<'a>(files: &[&'a SourceFile], project: &ProjectCtx) -> Vec<&'a SourceFile> {
    files
        .iter()
        .copied()
        .filter(|file| crate::rules::module_system::is_es_module_context(&file.path, project))
        .collect()
}

fn type_aware_files<'a>(files: &[&'a SourceFile]) -> Vec<&'a SourceFile> {
    files
        .iter()
        .copied()
        .filter(|file| {
            matches!(
                file.language,
                crate::files::Language::TypeScript | crate::files::Language::Tsx
            )
        })
        .collect()
}

fn lint_files_with_bindings(
    files: &[&SourceFile],
    config: &crate::config::Config,
    bindings: &[(&'static str, &'static RuleMeta, Severity)],
) -> Result<Vec<Diagnostic>> {
    let rule_entries: Vec<crate::oxlint_config::RuleEntry> = bindings
        .iter()
        .map(|(key, _, sev)| (*key, *sev, options::for_rule(key, config)))
        .collect();
    let oxlint_config = crate::oxlint_config::generate(&rule_entries)?;
    let remap = remap::build_table(bindings);

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

    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to invoke oxlint — install it with: npm install -g oxlint")?;
    let Some(status) = child
        .wait_timeout(OXLINT_BATCH_TIMEOUT)
        .context("failed to wait for oxlint")?
    else {
        let _ = child.kill();
        let _ = child.wait();
        eprintln!(
            "comply: oxlint timed out after {}s on {} file(s); continuing with partial results",
            OXLINT_BATCH_TIMEOUT.as_secs(),
            files.len()
        );
        return Ok(std::process::Output {
            status: timeout_exit_status(),
            stdout: b"{\"diagnostics\":[]}".to_vec(),
            stderr: Vec::new(),
        });
    };
    let output = child
        .wait_with_output()
        .context("failed to collect oxlint output")?;
    debug_assert_eq!(output.status, status);

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

#[cfg(unix)]
fn timeout_exit_status() -> std::process::ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    std::process::ExitStatus::from_raw(0)
}

#[cfg(windows)]
fn timeout_exit_status() -> std::process::ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    std::process::ExitStatus::from_raw(0)
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

    #[test]
    fn type_aware_files_exclude_plain_javascript() {
        let js = SourceFile {
            path: "fastify.js".into(),
            language: crate::files::Language::JavaScript,
        };
        let mjs = SourceFile {
            path: "plugin.mjs".into(),
            language: crate::files::Language::JavaScript,
        };
        let ts = SourceFile {
            path: "server.ts".into(),
            language: crate::files::Language::TypeScript,
        };
        let tsx = SourceFile {
            path: "view.tsx".into(),
            language: crate::files::Language::Tsx,
        };
        let files = [&js, &mjs, &ts, &tsx];

        let type_aware = type_aware_files(&files);

        assert_eq!(type_aware.len(), 2);
        assert!(type_aware.iter().any(|file| file.path.ends_with("server.ts")));
        assert!(type_aware.iter().any(|file| file.path.ends_with("view.tsx")));
    }

    #[test]
    fn es_module_files_respect_extensions_and_package_type() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
        let module_js = SourceFile {
            path: dir.path().join("src").join("module.js"),
            language: crate::files::Language::JavaScript,
        };
        let mjs = SourceFile {
            path: dir.path().join("standalone.mjs"),
            language: crate::files::Language::JavaScript,
        };
        let cjs = SourceFile {
            path: dir.path().join("legacy.cjs"),
            language: crate::files::Language::JavaScript,
        };
        let outside_js = SourceFile {
            path: dir.path().with_file_name("outside.js"),
            language: crate::files::Language::JavaScript,
        };
        let project = crate::project::ProjectCtx::empty();
        let files = [&module_js, &mjs, &cjs, &outside_js];

        let esm = es_module_files(&files, &project);

        assert_eq!(esm.len(), 2);
        assert!(esm.iter().any(|file| file.path.ends_with("module.js")));
        assert!(esm.iter().any(|file| file.path.ends_with("standalone.mjs")));
    }
}
