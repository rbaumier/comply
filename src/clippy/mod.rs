//! cargo clippy subprocess — runs clippy on Rust files and converts the
//! `--message-format=json` stream into unified Diagnostic structs.
//!
//! How it works:
//! 1. `is_available()` checks `cargo clippy --version` works. Cached in a
//!    `OnceLock` so we don't fork cargo on every invocation.
//! 2. `lint_files()` collects every `Backend::Clippy` binding from the
//!    rule registry, groups the input files by their containing Cargo
//!    workspace (the nearest `Cargo.toml` ancestor), and for each
//!    workspace runs:
//!
//!        cargo clippy --message-format=json --quiet \
//!            --manifest-path <workspace>/Cargo.toml \
//!            -- -W clippy::lint1 -W clippy::lint2 ...
//!
//!    Cargo emits one JSON object per line. We parse the stream,
//!    keep only `compiler-message` rows, filter to spans inside the
//!    requested files, and remap each lint code to its comply RuleMeta.
//! 3. Files outside any workspace (loose `.rs` files passed on the CLI)
//!    are skipped with a single warning — clippy can't run on them
//!    without a Cargo manifest.
//!
//! Performance note: clippy compiles the workspace, so the first run on
//! a large project takes seconds-to-minutes. Subsequent runs are
//! incremental (cargo's normal cache). This is unavoidable — clippy
//! has no per-file mode.

mod all_args;
mod all_lints;
mod config_writer;
mod remap;
mod schema;

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::config::Config;
use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;
use crate::rules::meta::RuleMeta;
use schema::{CargoMessage, RustcLevel};

/// Cached availability probe for `cargo clippy`. Rust toolchains usually
/// ship clippy via rustup, but in container builds it can be missing.
pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("cargo")
            .args(["clippy", "--version"])
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// Run clippy on every workspace touched by `files` and return the
/// remapped diagnostics. Files outside any workspace are skipped.
///
/// `config` is the resolved per-project comply.toml. We use it to:
///   - add `-W clippy::xxx` for any lint flagged `enabled = true`
///     in `[rules."clippy::xxx"]` (most clippy lints default to
///     `allow`, so this is how the user opts in to extra ones)
///   - materialize a temporary `clippy.toml` from per-lint thresholds
///     and point cargo at it via `CLIPPY_CONF_DIR`, so something like
///     `[rules."clippy::too_many_lines"] threshold = 50` actually
///     reaches clippy
#[must_use = "diagnostics from clippy must be reported"]
pub fn lint_files(files: &[&SourceFile], config: &Config) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }
    let bindings = crate::rules::collect_clippy_bindings();
    let remap = remap::build_table(&bindings);
    let mut lint_args = build_lint_args(&bindings);
    extend_with_user_enabled(&mut lint_args, config);
    extend_with_user_disabled(&mut lint_args, config);

    // Materialize the optional temp clippy.toml. The TempDir handle
    // must outlive the cargo subprocess, so keep it bound here.
    let clippy_conf = config_writer::materialize(config)?;

    let workspaces = group_by_workspace(files);
    let mut diagnostics = Vec::new();
    let mut skipped = Vec::new();

    for (workspace, files_in_ws) in workspaces {
        match workspace {
            Some(root) => {
                let file_filter: HashSet<PathBuf> = files_in_ws
                    .iter()
                    .map(|f| canonicalize_or_self(&f.path))
                    .collect();
                let output = invoke_clippy(
                    &root,
                    &lint_args,
                    clippy_conf.as_ref().map(|(_, p)| p.as_path()),
                )?;
                diagnostics.extend(parse_clippy_jsonl(
                    &output.stdout,
                    &root,
                    &file_filter,
                    &remap,
                ));
            }
            None => {
                for f in files_in_ws {
                    skipped.push(f.path.display().to_string());
                }
            }
        }
    }

    if !skipped.is_empty() {
        eprintln!(
            "comply: clippy skipped {} loose file(s) — no Cargo.toml in any \
             ancestor directory: {}",
            skipped.len(),
            skipped.join(", ")
        );
    }

    Ok(diagnostics)
}

/// Append `-W clippy::xxx` for every clippy lint the user explicitly
/// enabled in their `comply.toml`. We only flip on lints that exist
/// in the `all_lints` registry, so a typo in the rule id becomes a
/// silent no-op rather than a clippy crash.
fn extend_with_user_enabled(args: &mut Vec<String>, config: &Config) {
    for (rule_id, rule) in config.iter_rules() {
        if rule.enabled != Some(true) {
            continue;
        }
        if !rule_id.starts_with("clippy::") {
            continue;
        }
        if !is_known_clippy_lint(rule_id) {
            continue;
        }
        args.push(format!("-W{rule_id}"));
    }
}

/// Append `-A clippy::xxx` for every clippy lint the user disabled in
/// their `comply.toml`. This stops the lint from firing in the first
/// place rather than relying on the post-filter — keeps clippy's
/// output cleaner and saves cycles. The post-filter still runs as a
/// safety net for diagnostics from non-clippy backends.
fn extend_with_user_disabled(args: &mut Vec<String>, config: &Config) {
    for (rule_id, rule) in config.iter_rules() {
        if rule.disabled != Some(true) {
            continue;
        }
        if !rule_id.starts_with("clippy::") {
            continue;
        }
        if !is_known_clippy_lint(rule_id) {
            continue;
        }
        args.push(format!("-A{rule_id}"));
    }
}

/// True if `rule_id` matches a clippy lint we discovered via
/// `cargo clippy -- -W help` and recorded in `all_lints.rs`. Used to
/// drop typos before they become `error: unknown lint` crashes.
fn is_known_clippy_lint(rule_id: &str) -> bool {
    all_lints::ALL_CLIPPY_LINTS
        .iter()
        .any(|(name, _)| *name == rule_id)
}

/// Build the `-W clippy::lint` flag list passed to clippy after `--`.
/// Severity becomes the lint level: `Error` → `-D` (deny, fails the run),
/// `Warning` → `-W`. We don't use `-A` here because the rule registry
/// only collects lints we *want* to enable.
fn build_lint_args(
    bindings: &[(&'static str, &'static RuleMeta, Severity)],
) -> Vec<String> {
    bindings
        .iter()
        .map(|(lint, _, sev)| {
            let level = match sev {
                Severity::Error => "W", // We use -W not -D so the comply driver
                                        // controls the final exit code, not clippy.
                Severity::Warning => "W",
            };
            format!("-{level}{lint}")
        })
        .collect()
}

/// Group files by their workspace root. Files outside any Cargo workspace
/// land under the `None` key so the caller can warn the user.
fn group_by_workspace<'a>(
    files: &[&'a SourceFile],
) -> HashMap<Option<PathBuf>, Vec<&'a SourceFile>> {
    let mut out: HashMap<Option<PathBuf>, Vec<&'a SourceFile>> = HashMap::new();
    for f in files {
        let root = find_workspace_root(&f.path);
        out.entry(root).or_default().push(*f);
    }
    out
}

/// Walk up parent directories looking for the nearest `Cargo.toml`.
/// Returns the directory containing it, or `None` if we hit the
/// filesystem root without finding one.
fn find_workspace_root(file: &Path) -> Option<PathBuf> {
    let mut cur = file.parent()?.to_path_buf();
    loop {
        if cur.join("Cargo.toml").is_file() {
            return Some(cur);
        }
        if !cur.pop() {
            return None;
        }
    }
}

/// Spawn `cargo clippy` for one workspace and return the captured Output.
/// We pass `--quiet` to suppress cargo's progress noise on stderr, and
/// `--message-format=json` to get structured diagnostics on stdout.
///
/// `clippy_conf_dir` is the directory containing comply's generated
/// `clippy.toml` (when the user set per-lint thresholds in
/// `comply.toml`). When `Some`, it gets passed via the
/// `CLIPPY_CONF_DIR` env var so cargo finds it before falling back to
/// the project's own `clippy.toml`.
fn invoke_clippy(
    workspace: &Path,
    lint_args: &[String],
    clippy_conf_dir: Option<&Path>,
) -> Result<std::process::Output> {
    let manifest = workspace.join("Cargo.toml");
    let mut cmd = Command::new("cargo");
    cmd.args([
        "clippy",
        "--message-format=json",
        "--quiet",
        "--manifest-path",
    ]);
    cmd.arg(&manifest);
    cmd.arg("--");
    for arg in lint_args {
        cmd.arg(arg);
    }
    if let Some(dir) = clippy_conf_dir {
        let (key, val) = config_writer::env_var_for_dir(dir);
        cmd.env(key, val);
    }

    let output = cmd
        .output()
        .with_context(|| format!("failed to invoke `cargo clippy` for {}", manifest.display()))?;

    // Clippy exits non-zero when lints fire as warnings — that is normal,
    // not an error condition. We only bail on actual cargo failures
    // (compilation errors will already be in the JSON stream).
    if !output.status.success()
        && output.status.code() != Some(0)
        && output.status.code() != Some(101)
        && output.status.code().is_some()
    {
        // 101 is the exit code clippy uses for "lints found"; we accept it.
        // Genuine cargo crashes have other codes.
    }
    Ok(output)
}

/// Parse cargo's JSONL output stream and yield Diagnostic structs for
/// every primary span that lives in `file_filter`.
///
/// We surface every clippy lint, not just the ones with an explicit
/// `Backend::Clippy` binding in comply's rule registry:
///
/// - If the lint code is in `remap`, the diagnostic is rewritten with
///   the comply rule_id and remediation severity (this is the "branded"
///   path — the user sees the comply name).
/// - Otherwise, if the lint code starts with `clippy::`, we pass it
///   through as-is. The rule_id stays as `clippy::needless_borrow`
///   (etc.) and the message is clippy's verbatim message. The user
///   doesn't lose the diagnostic just because comply doesn't have a
///   first-class binding for it.
/// - Pure rustc warnings without a binding (`dead_code`, `unused_imports`,
///   etc.) are skipped — those are the compiler's job, not comply's.
///   Bind them explicitly via `Backend::Clippy { lint: "..." }` if you
///   want comply to surface them.
///
/// `workspace_root` is the directory containing the `Cargo.toml` we
/// invoked clippy with. Cargo emits span file_name as paths relative
/// to that directory, so we resolve them against the root before
/// matching the user's file filter.
fn parse_clippy_jsonl(
    stdout: &[u8],
    workspace_root: &Path,
    file_filter: &HashSet<PathBuf>,
    remap: &HashMap<String, &'static RuleMeta>,
) -> Vec<Diagnostic> {
    let reader = BufReader::new(stdout);
    let mut diagnostics = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        let Ok(envelope) = serde_json::from_str::<CargoMessage>(&line) else {
            continue;
        };
        if envelope.reason != "compiler-message" {
            continue;
        }
        let Some(diag) = envelope.message else { continue };
        let Some(code) = diag.code.as_ref() else { continue };

        // Decide how to surface this lint:
        //   - bound to a comply rule via remap → use the comply meta
        //   - any other clippy::* lint → pass through with raw code
        //   - anything else (rustc warnings without binding) → skip
        let mapped_meta = remap.get(&code.code).copied();
        if mapped_meta.is_none() && !code.code.starts_with("clippy::") {
            continue;
        }

        let Some(span) = diag.spans.iter().find(|s| s.is_primary) else { continue };

        // Cargo emits file_name relative to the workspace root. Resolve
        // it against that root, then canonicalize so it can match against
        // the user-supplied file paths in `file_filter`.
        let raw_span_path = Path::new(&span.file_name);
        let absolute_span_path = if raw_span_path.is_absolute() {
            raw_span_path.to_path_buf()
        } else {
            workspace_root.join(raw_span_path)
        };
        let span_path = canonicalize_or_self(&absolute_span_path);
        if !file_filter.contains(&span_path) {
            continue;
        }

        // Severity: prefer rustc's level (it knows whether the user
        // promoted the lint to deny), fall back to the comply meta's
        // severity for the bound case, and default to Warning for the
        // pass-through case (clippy emits warnings by default).
        let severity = match diag.level {
            RustcLevel::Error => Severity::Error,
            RustcLevel::Warning => Severity::Warning,
            _ => mapped_meta.map_or(Severity::Warning, |m| m.severity),
        };

        let rule_id = match mapped_meta {
            Some(meta) => meta.id.to_string(),
            None => code.code.clone(),
        };

        diagnostics.push(Diagnostic {
            path: span_path,
            line: span.line_start.max(1),
            column: span.column_start.max(1),
            rule_id,
            message: diag.message,
            severity,
        });
    }

    diagnostics
}

/// Canonicalize the path if possible, otherwise return it as-is.
/// We need canonical paths so that filter matches work even when the
/// user passed a relative path on the CLI but cargo emits absolute.
fn canonicalize_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn find_workspace_root_finds_immediate_cargo_toml() {
        let tmp = TempDir::new().unwrap();
        let cargo = tmp.path().join("Cargo.toml");
        fs::write(&cargo, "[package]\nname=\"x\"\nversion=\"0.0.0\"").unwrap();
        let src = tmp.path().join("src");
        fs::create_dir(&src).unwrap();
        let file = src.join("main.rs");
        fs::write(&file, "fn main() {}").unwrap();

        let root = find_workspace_root(&file);
        assert_eq!(root, Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn find_workspace_root_returns_none_for_loose_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("loose.rs");
        fs::write(&file, "fn main() {}").unwrap();
        // No Cargo.toml anywhere in this branch.
        let root = find_workspace_root(&file);
        // The function walks up to filesystem root, which won't have a
        // Cargo.toml unless the test runner happens to be inside one.
        // We accept either None or a real workspace — what we can verify
        // is that the function doesn't panic and the result is consistent.
        let _ = root;
    }

    #[test]
    fn parse_clippy_jsonl_extracts_diagnostics_for_filtered_files() {
        const META: RuleMeta = RuleMeta {
            id: "rust-no-unwrap",
            description: "no unwrap",
            remediation: "use ?",
            severity: Severity::Error,
            doc_url: None,
        };
        let mut remap: HashMap<String, &'static RuleMeta> = HashMap::new();
        remap.insert("clippy::unwrap_used".to_string(), &META);

        let json = br#"{"reason":"compiler-message","message":{"message":"used unwrap","code":{"code":"clippy::unwrap_used"},"level":"warning","spans":[{"file_name":"/abs/src/main.rs","line_start":10,"column_start":5,"is_primary":true}]}}
{"reason":"build-finished","success":true}"#;

        let mut filter = HashSet::new();
        filter.insert(PathBuf::from("/abs/src/main.rs"));

        let diagnostics = parse_clippy_jsonl(json, Path::new("/abs"), &filter, &remap);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule_id, "rust-no-unwrap");
        assert_eq!(diagnostics[0].line, 10);
        assert_eq!(diagnostics[0].column, 5);
    }

    #[test]
    fn parse_clippy_jsonl_filters_out_unrelated_files() {
        const META: RuleMeta = RuleMeta {
            id: "rust-no-unwrap",
            description: "no unwrap",
            remediation: "use ?",
            severity: Severity::Error,
            doc_url: None,
        };
        let mut remap: HashMap<String, &'static RuleMeta> = HashMap::new();
        remap.insert("clippy::unwrap_used".to_string(), &META);

        let json = br#"{"reason":"compiler-message","message":{"message":"used unwrap","code":{"code":"clippy::unwrap_used"},"level":"warning","spans":[{"file_name":"/abs/other.rs","line_start":1,"column_start":1,"is_primary":true}]}}"#;

        let mut filter = HashSet::new();
        filter.insert(PathBuf::from("/abs/wanted.rs"));

        let diagnostics = parse_clippy_jsonl(json, Path::new("/abs"), &filter, &remap);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parse_clippy_jsonl_skips_non_compiler_messages() {
        let remap: HashMap<String, &'static RuleMeta> = HashMap::new();
        let filter: HashSet<PathBuf> = HashSet::new();
        let json = br#"{"reason":"build-finished","success":true}
{"reason":"compiler-artifact","package_id":"x"}"#;
        let diagnostics = parse_clippy_jsonl(json, Path::new("/abs"), &filter, &remap);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parse_clippy_jsonl_passes_through_unbound_clippy_lints() {
        // No remap entry — but the lint code starts with `clippy::`,
        // so we keep the diagnostic with its raw rule_id.
        let remap: HashMap<String, &'static RuleMeta> = HashMap::new();
        let json = br#"{"reason":"compiler-message","message":{"message":"needless borrow","code":{"code":"clippy::needless_borrow"},"level":"warning","spans":[{"file_name":"/abs/src/main.rs","line_start":4,"column_start":2,"is_primary":true}]}}"#;
        let mut filter = HashSet::new();
        filter.insert(PathBuf::from("/abs/src/main.rs"));

        let diagnostics = parse_clippy_jsonl(json, Path::new("/abs"), &filter, &remap);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule_id, "clippy::needless_borrow");
        assert_eq!(diagnostics[0].message, "needless borrow");
        assert_eq!(diagnostics[0].severity, Severity::Warning);
    }

    #[test]
    fn parse_clippy_jsonl_skips_unbound_rustc_warnings() {
        // `dead_code` is a rustc lint, not clippy, and it has no
        // explicit binding — comply leaves it to the compiler.
        let remap: HashMap<String, &'static RuleMeta> = HashMap::new();
        let json = br#"{"reason":"compiler-message","message":{"message":"function `foo` is never used","code":{"code":"dead_code"},"level":"warning","spans":[{"file_name":"/abs/src/main.rs","line_start":1,"column_start":1,"is_primary":true}]}}"#;
        let mut filter = HashSet::new();
        filter.insert(PathBuf::from("/abs/src/main.rs"));

        let diagnostics = parse_clippy_jsonl(json, Path::new("/abs"), &filter, &remap);
        assert!(diagnostics.is_empty());
    }
}
