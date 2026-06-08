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

mod elysia_post_filter;
mod equal_probe_post_filter;
mod no_misused_spread_post_filter;
mod no_redundant_type_constituents_post_filter;
mod no_unnecessary_condition_exhaustiveness_post_filter;
mod promise_function_async_post_filter;
mod strict_void_return_post_filter;
mod options;
mod remap;
mod schema;

pub use options::for_rule as options_for;

use anyhow::{Context, Result, bail};
use rustc_hash::{FxHashMap, FxHashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
///
/// `type_aware` gates the tsgolint rule set (and the `--type-aware` flag that
/// requires oxlint-tsgolint to build the TypeScript program). Off by default
/// so the standard run stays syntactic and fast; the type-aware batches only
/// run when the caller opted in via `--type-aware`.
#[must_use = "diagnostics from oxlint must be reported"]
pub fn lint_files(
    files: &[&SourceFile],
    config: &crate::config::Config,
    project: &ProjectCtx,
    type_aware: bool,
    type_program_files: Option<&[&SourceFile]>,
) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }
    let oxlint_bindings = crate::rules::collect_oxlint_bindings();
    let tsgolint_bindings = if type_aware {
        crate::rules::collect_tsgolint_bindings()
    } else {
        Vec::new()
    };
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
        all.extend(lint_files_with_bindings(files, config, &standard_oxlint, false)?);
    }

    let type_aware_ts = type_aware_files(files);
    if !type_aware_ts.is_empty() && !standard_tsgolint.is_empty() {
        if let Some(program) = type_program_files {
            // Partial-scan: pass all project TS files so tsgolint builds a
            // complete type program (prevents no-unsafe-* FPs on imports from
            // unchanged files), then filter results back to the changed files.
            let program_ts = type_aware_files(program);
            if !program_ts.is_empty() {
                let rule_entries: Vec<crate::oxlint_config::RuleEntry<'_>> = standard_tsgolint
                    .iter()
                    .map(|(key, _, sev)| (*key, *sev, options::for_rule(key, config)))
                    .collect();
                let oxlint_config = crate::oxlint_config::generate(&rule_entries)?;
                let remap = remap::build_table(&standard_tsgolint);
                let output = invoke_oxlint(&program_ts, Some(oxlint_config.path()), true)?;
                let all_diags = parse_json_bytes(&output.stdout, &output.stderr, &remap)?;
                let changed = changed_path_set(&type_aware_ts);
                all.extend(all_diags.into_iter().filter(|d| changed.contains(d.path.as_ref())));
            }
        } else {
            all.extend(lint_files_with_bindings(
                &type_aware_ts,
                config,
                &standard_tsgolint,
                true,
            )?);
        }
    }

    let esm = es_module_files(files, project);
    if !esm.is_empty() && !module_aware_oxlint.is_empty() {
        all.extend(lint_files_with_bindings(
            &esm,
            config,
            &module_aware_oxlint,
            false,
        )?);
    }

    let type_aware_esm = type_aware_files(&esm);
    if !type_aware_esm.is_empty() && !module_aware_tsgolint.is_empty() {
        if let Some(program) = type_program_files {
            let program_esm = type_aware_files(&es_module_files(program, project));
            if !program_esm.is_empty() {
                let rule_entries: Vec<crate::oxlint_config::RuleEntry<'_>> = module_aware_tsgolint
                    .iter()
                    .map(|(key, _, sev)| (*key, *sev, options::for_rule(key, config)))
                    .collect();
                let oxlint_config = crate::oxlint_config::generate(&rule_entries)?;
                let remap = remap::build_table(&module_aware_tsgolint);
                let output = invoke_oxlint(&program_esm, Some(oxlint_config.path()), true)?;
                let all_diags = parse_json_bytes(&output.stdout, &output.stderr, &remap)?;
                let changed = changed_path_set(&type_aware_esm);
                all.extend(all_diags.into_iter().filter(|d| changed.contains(d.path.as_ref())));
            }
        } else {
            all.extend(lint_files_with_bindings(
                &type_aware_esm,
                config,
                &module_aware_tsgolint,
                true,
            )?);
        }
    }

    let filters = crate::rules::collect_delegated_post_filters();
    if !filters.is_empty() {
        let mut source_cache: FxHashMap<PathBuf, Option<String>> = FxHashMap::default();
        all.retain(|d| {
            let Some(filter_vec) = filters.get(d.rule_id.as_ref()) else {
                return true;
            };
            let source = source_cache
                .entry(d.path.to_path_buf())
                .or_insert_with(|| std::fs::read_to_string(d.path.as_ref()).ok())
                .as_deref();
            filter_vec.iter().all(|f| f.keep(d, source))
        });
    }

    Ok(all)
}

fn changed_path_set(files: &[&SourceFile]) -> FxHashSet<PathBuf> {
    files
        .iter()
        .map(|f| std::fs::canonicalize(&f.path).unwrap_or_else(|_| f.path.clone()))
        .collect()
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
    type_aware: bool,
) -> Result<Vec<Diagnostic>> {
    let rule_entries: Vec<crate::oxlint_config::RuleEntry> = bindings
        .iter()
        .map(|(key, _, sev)| (*key, *sev, options::for_rule(key, config)))
        .collect();
    let oxlint_config = crate::oxlint_config::generate(&rule_entries)?;
    let remap = remap::build_table(bindings);

    let mut all = Vec::with_capacity(files.len());
    for batch in files.chunks(FILES_PER_BATCH) {
        let output = invoke_oxlint(batch, Some(oxlint_config.path()), type_aware)?;
        all.extend(parse_json_bytes(&output.stdout, &output.stderr, &remap)?);
    }
    Ok(all)
}

/// Spawn oxlint as a subprocess and validate exit status.
///
/// `--type-aware` is only passed for tsgolint batches: it forces oxlint to
/// build the TypeScript program (slow), so the syntactic-rule batches run
/// without it.
fn invoke_oxlint(
    files: &[&SourceFile],
    config_path: Option<&Path>,
    type_aware: bool,
) -> Result<std::process::Output> {
    let mut cmd = Command::new("oxlint");
    cmd.args(["--format", "json"]);
    if type_aware {
        cmd.arg("--type-aware");
    }
    if let Some(cfg) = config_path {
        cmd.arg("-c").arg(cfg);
    }
    // `--` terminates option parsing so file paths starting with `-` are not
    // interpreted as flags by oxlint.
    cmd.arg("--");
    for f in files {
        cmd.arg(&f.path);
    }

    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to invoke oxlint — install it with: npm install -g oxlint")?;
    let Some(output) = drain_and_wait(child, OXLINT_BATCH_TIMEOUT)? else {
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

/// Wait for `child` up to `timeout`, draining its stdout/stderr on dedicated
/// threads. Returns `Ok(None)` if the child was killed for exceeding `timeout`.
///
/// The threads are load-bearing: `wait_timeout` only polls the process and
/// never reads the pipes, so a child that writes past the OS pipe buffer
/// (~64 KiB) blocks on `write()` and can never exit — a deadlock that surfaces
/// as a spurious timeout on any project with enough diagnostics to overflow it.
fn drain_and_wait(
    mut child: std::process::Child,
    timeout: Duration,
) -> Result<Option<std::process::Output>> {
    let mut stdout_pipe = child.stdout.take().expect("child stdout must be piped");
    let mut stderr_pipe = child.stderr.take().expect("child stderr must be piped");
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });

    let Some(status) = child
        .wait_timeout(timeout)
        .context("failed to wait for oxlint")?
    else {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
        return Ok(None);
    };

    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    Ok(Some(std::process::Output {
        status,
        stdout,
        stderr,
    }))
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

    // Manual integration test for issue #755:
    // 1. Create project with A.ts (unchanged, exports typed hook) and B.tsx (changed, imports A)
    // 2. Run `comply --working-tree --type-aware` on B.tsx only
    // 3. Expect: no no-unsafe-* errors in B.tsx
    // Reproducer from issue: mutations.ts with comply-ignore-file: unused-file

    #[test]
    fn changed_path_set_retains_changed_files_and_drops_program_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let changed_path = dir.path().join("B.tsx");
        let unchanged_path = dir.path().join("A.ts");
        std::fs::write(&changed_path, "").unwrap();
        std::fs::write(&unchanged_path, "").unwrap();

        let changed_sf = SourceFile {
            path: changed_path.clone(),
            language: crate::files::Language::Tsx,
        };
        let set = changed_path_set(&[&changed_sf]);

        // B.tsx (changed) resolves and is present
        assert!(
            set.contains(&std::fs::canonicalize(&changed_path).unwrap()),
            "changed file must be in set"
        );
        // A.ts (unchanged program file) is absent
        assert!(
            !set.contains(&std::fs::canonicalize(&unchanged_path).unwrap()),
            "unchanged file must not be in set"
        );

        // Diagnostics from B.tsx survive; diagnostics from A.ts are dropped.
        // oxlint canonicalizes paths, so diagnostics carry canonical paths.
        let make_diag = |path: &std::path::Path| {
            let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            crate::diagnostic::Diagnostic {
                path: std::sync::Arc::from(canon.as_path()),
                line: 1,
                column: 1,
                rule_id: std::borrow::Cow::Borrowed("no-unsafe-assignment"),
                message: "unsafe".into(),
                severity: crate::diagnostic::Severity::Error,
                span: None,
            }
        };
        let diags = vec![make_diag(&changed_path), make_diag(&unchanged_path)];
        let filtered: Vec<_> = diags
            .into_iter()
            .filter(|d| set.contains(d.path.as_ref()))
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0].path.as_ref(),
            std::fs::canonicalize(&changed_path).unwrap().as_path()
        );
    }

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
    fn drain_and_wait_does_not_deadlock_on_output_exceeding_pipe_buffer() {
        // A child that writes ~200 KiB — far past the ~64 KiB OS pipe buffer —
        // then exits 0. Without draining the pipes on threads, oxlint blocks on
        // write() and `wait_timeout` never sees it exit (issue: svix-webhooks
        // timed out at 45s despite finishing in <1s). It must complete, not
        // time out, and its full output must be captured.
        let child = Command::new("sh")
            .args([
                "-c",
                "i=0; while [ $i -lt 4000 ]; do printf '%050d\\n' $i; i=$((i + 1)); done",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn sh");

        let output = drain_and_wait(child, Duration::from_secs(30))
            .expect("wait must not error")
            .expect("child must complete, not time out");

        assert!(output.status.success());
        assert!(
            output.stdout.len() > 200_000,
            "expected full output, got {} bytes",
            output.stdout.len()
        );
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
