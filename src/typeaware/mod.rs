//! Type-aware analysis phase.
//!
//! Runs comply's custom type-aware rules (`Backend::TypeAware`) that need a
//! resolved TypeScript program — type queries, structural comparison — which
//! no in-process tree-sitter pass can answer. comply drives a TypeScript
//! checker (typescript-go via `@typescript/native-preview`) through a Node
//! sidecar: it sends the tsconfig + file list + enabled rule ids, the sidecar
//! builds the program once and returns the violations as JSON.
//!
//! Only invoked when `--type-aware` is passed (see `main::lint_typescript`),
//! so the standard run stays AstCheck-only and keeps its sub-60s budget. The
//! sidecar phase accepts a much higher cost (building the program dominates).
//!
//! Graceful degradation: a missing `node` or `@typescript/native-preview`, a
//! missing tsconfig, or a sidecar timeout prints a one-line notice to stderr
//! and yields no diagnostics rather than failing the whole run.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use wait_timeout::ChildExt;

use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::files::SourceFile;

/// The sidecar is embedded so the binary is self-contained; it's written to a
/// temp file per run and executed with `node`.
const SIDECAR_SRC: &str = include_str!("sidecar.mjs");

/// Generous ceiling — type-aware analysis builds the full TS program, which is
/// the expensive part. Anything past this is treated as a hang, not a result.
const SIDECAR_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Deserialize)]
struct SidecarResponse {
    diagnostics: Vec<SidecarDiag>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
struct SidecarDiag {
    file: String,
    line: usize,
    column: usize,
    rule: String,
    message: String,
}

/// Run the enabled custom type-aware rules over `files`.
#[must_use = "diagnostics from the type-aware sidecar must be reported"]
pub fn lint_files(files: &[&SourceFile], config: &Config) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }

    // A rule runs when it's enabled for at least one file; per-file / per-glob
    // disables are applied later by `apply_config_filters`.
    let metas = crate::rules::collect_type_aware_bindings();
    let enabled: Vec<&'static crate::rules::meta::RuleMeta> = metas
        .into_iter()
        .filter(|m| files.iter().any(|f| config.is_rule_enabled(m.id, &f.path)))
        .collect();
    if enabled.is_empty() {
        return Ok(vec![]);
    }

    if !node_available() {
        eprintln!(
            "comply: --type-aware needs Node.js on PATH — skipping type-aware rules."
        );
        return Ok(vec![]);
    }

    let Some(tsconfig) = find_tsconfig(files) else {
        eprintln!(
            "comply: --type-aware found no tsconfig.json — skipping type-aware rules."
        );
        return Ok(vec![]);
    };
    let project_dir = tsconfig
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    // The sidecar identifies files by their canonical path; keep a map back to
    // the path comply discovered so reported diagnostics match the rest of the
    // run (config filtering, diff-only normalization all key on that path).
    let mut canon_to_orig: rustc_hash::FxHashMap<String, PathBuf> = rustc_hash::FxHashMap::default();
    let mut abs_files: Vec<String> = Vec::with_capacity(files.len());
    for f in files {
        if let Ok(canon) = std::fs::canonicalize(&f.path) {
            let canon = canon.to_string_lossy().into_owned();
            canon_to_orig.insert(canon.clone(), f.path.clone());
            abs_files.push(canon);
        }
    }
    let rule_ids: Vec<&str> = enabled.iter().map(|m| m.id).collect();
    let request = serde_json::json!({
        "tsconfig": tsconfig.to_string_lossy(),
        "files": abs_files,
        "rules": rule_ids,
    })
    .to_string();

    let response = match run_sidecar(&project_dir, &request)? {
        Some(r) => r,
        None => return Ok(vec![]),
    };

    if let Some(err) = response.error.as_deref() {
        report_sidecar_error(err);
        return Ok(vec![]);
    }

    Ok(map_diagnostics(
        response.diagnostics,
        &enabled,
        config,
        &canon_to_orig,
    ))
}

/// Whether `node` is on PATH. Cached for the process lifetime.
fn node_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("node")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// Path to the nearest `tsconfig.json` walking up from the first file's
/// directory. `walk_up_finding` yields the containing directory, so the
/// filename is appended.
fn find_tsconfig(files: &[&SourceFile]) -> Option<PathBuf> {
    let first = files.first()?;
    let start = std::fs::canonicalize(&first.path)
        .ok()?
        .parent()?
        .to_path_buf();
    let dir = crate::project::walk_up_finding(&start, "tsconfig.json")?;
    Some(dir.join("tsconfig.json"))
}

/// Write the sidecar to a temp file, run it with the request on stdin, and
/// parse its JSON response. Returns `None` (with a stderr notice) on spawn
/// failure or timeout.
fn run_sidecar(project_dir: &Path, request: &str) -> Result<Option<SidecarResponse>> {
    let sidecar_path =
        std::env::temp_dir().join(format!("comply-typeaware-{}.mjs", std::process::id()));
    std::fs::write(&sidecar_path, SIDECAR_SRC)
        .with_context(|| format!("failed to write sidecar to {}", sidecar_path.display()))?;
    let _cleanup = TempFile(&sidecar_path);

    let mut child = Command::new("node")
        .arg(&sidecar_path)
        .current_dir(project_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn node for the type-aware sidecar")?;

    child
        .stdin
        .take()
        .context("sidecar stdin unavailable")?
        .write_all(request.as_bytes())
        .context("failed to send request to the type-aware sidecar")?;

    let Some(_status) = child
        .wait_timeout(SIDECAR_TIMEOUT)
        .context("failed to wait for the type-aware sidecar")?
    else {
        let _ = child.kill();
        let _ = child.wait();
        eprintln!(
            "comply: type-aware sidecar timed out after {}s — skipping type-aware rules.",
            SIDECAR_TIMEOUT.as_secs()
        );
        return Ok(None);
    };

    let mut stdout = String::new();
    if let Some(mut out) = child.stdout.take() {
        out.read_to_string(&mut stdout)
            .context("failed to read sidecar output")?;
    }

    let response: SidecarResponse = serde_json::from_str(&stdout).with_context(|| {
        let mut stderr = String::new();
        if let Some(mut err) = child.stderr.take() {
            let _ = err.read_to_string(&mut stderr);
        }
        format!("failed to parse type-aware sidecar output. sidecar stderr: {stderr}")
    })?;
    Ok(Some(response))
}

fn report_sidecar_error(err: &str) {
    if err == "package-not-found" {
        eprintln!(
            "comply: --type-aware needs the typescript-go API — install it in the project with: \
             npm install --save-dev @typescript/native-preview"
        );
    } else {
        eprintln!("comply: type-aware sidecar error ({err}) — skipping type-aware rules.");
    }
}

/// Map sidecar diagnostics into comply diagnostics, applying the rule's
/// configured severity (or its default) and ignoring any unknown rule id.
fn map_diagnostics(
    diags: Vec<SidecarDiag>,
    enabled: &[&'static crate::rules::meta::RuleMeta],
    config: &Config,
    canon_to_orig: &rustc_hash::FxHashMap<String, PathBuf>,
) -> Vec<Diagnostic> {
    diags
        .into_iter()
        .filter_map(|d| {
            let meta = enabled.iter().find(|m| m.id == d.rule)?;
            let severity = config.severity_for(meta.id).unwrap_or(meta.severity);
            let path = canon_to_orig
                .get(&d.file)
                .cloned()
                .unwrap_or_else(|| PathBuf::from(&d.file));
            Some(Diagnostic {
                path: std::sync::Arc::from(path.as_path()),
                line: d.line.max(1),
                column: d.column.max(1),
                rule_id: std::borrow::Cow::Borrowed(meta.id),
                message: d.message,
                severity,
                span: None,
            })
        })
        .collect()
}

/// Removes the temp sidecar file on drop (best-effort).
struct TempFile<'a>(&'a Path);

impl Drop for TempFile<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_static_config;
    use crate::diagnostic::Severity;
    use crate::rules::meta::RuleMeta;

    const REDUNDANT_NULLISH: &str = "no-redundant-nullish-coalescing-null";

    fn meta() -> &'static RuleMeta {
        Box::leak(Box::new(RuleMeta {
            id: REDUNDANT_NULLISH,
            description: "d",
            remediation: "r",
            severity: Severity::Warning,
            doc_url: None,
            categories: &["typescript", "type-aware"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        }))
    }

    #[test]
    fn map_diagnostics_remaps_canonical_path_to_discovered() {
        let m = meta();
        let mut canon = rustc_hash::FxHashMap::default();
        canon.insert(
            "/private/tmp/proj/src/a.ts".to_string(),
            PathBuf::from("/tmp/proj/src/a.ts"),
        );
        let diags = vec![SidecarDiag {
            file: "/private/tmp/proj/src/a.ts".to_string(),
            line: 6,
            column: 19,
            rule: REDUNDANT_NULLISH.to_string(),
            message: "msg".to_string(),
        }];
        let out = map_diagnostics(diags, &[m], default_static_config(), &canon);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path.to_string_lossy(), "/tmp/proj/src/a.ts");
        assert_eq!(out[0].line, 6);
        assert_eq!(out[0].column, 19);
        assert_eq!(out[0].rule_id, REDUNDANT_NULLISH);
    }

    #[test]
    fn map_diagnostics_drops_unknown_rule_ids() {
        let m = meta();
        let canon = rustc_hash::FxHashMap::default();
        let diags = vec![SidecarDiag {
            file: "/tmp/x.ts".to_string(),
            line: 1,
            column: 1,
            rule: "some-other-rule".to_string(),
            message: "msg".to_string(),
        }];
        let out = map_diagnostics(diags, &[m], default_static_config(), &canon);
        assert!(out.is_empty(), "unknown rule ids must be ignored");
    }

    #[test]
    fn map_diagnostics_falls_back_to_reported_path_when_unmapped() {
        let m = meta();
        let canon = rustc_hash::FxHashMap::default();
        let diags = vec![SidecarDiag {
            file: "/abs/unmapped.ts".to_string(),
            line: 2,
            column: 3,
            rule: REDUNDANT_NULLISH.to_string(),
            message: "msg".to_string(),
        }];
        let out = map_diagnostics(diags, &[m], default_static_config(), &canon);
        assert_eq!(out[0].path.to_string_lossy(), "/abs/unmapped.ts");
    }

    /// The custom type-aware rules are registered with `Backend::TypeAware` so
    /// they surface in `comply list`/`catalog` and feed the sidecar phase.
    #[test]
    fn custom_type_aware_rules_are_registered() {
        let metas = crate::rules::collect_type_aware_bindings();
        for id in [REDUNDANT_NULLISH, "no-duplicate-type-definition"] {
            assert!(
                metas.iter().any(|m| m.id == id),
                "expected {id} among type-aware bindings"
            );
        }
    }
}
