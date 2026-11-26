//! @generated — cargo clippy subprocess — runs clippy on Rust files and converts the
//! `--message-format=json` stream into unified Diagnostic structs.
//!
//! How it works:
//! 1. `is_available()` checks `cargo clippy --version` works. Cached in a
//!    `OnceLock` so we don't fork cargo on every invocation.
//! 2. `lint_files()` collects every `Backend::Clippy` binding from the
//!    rule registry, groups the input files by their containing Cargo
//!    workspace root, and for each workspace runs:
//!
//!        cargo clippy <--workspace | -p <owning package>...> \
//!            --all-targets --keep-going --message-format=json --quiet \
//!            --manifest-path <workspace>/Cargo.toml \
//!            -- -W clippy::lint1 -W clippy::lint2 ...
//!
//!    Cargo emits one JSON object per line. We parse the stream,
//!    keep only `compiler-message` rows, filter to spans inside the
//!    requested files, and remap each lint code to its comply RuleMeta.
//!    A run stopped by a denied lint gets two more (see [`Pass`]).
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
pub(crate) mod config_writer;
mod remap;
mod schema;

use anyhow::{Context, Result};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::config::Config;
use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;
use crate::rules::meta::RuleMeta;
use schema::{CargoMessage, RustcDiagnostic, RustcSpan};

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
///   - materialize a `clippy.toml` from per-lint thresholds and `[clippy]`
///     (or the project's own clippy.toml), and point cargo at it via `CLIPPY_CONF_DIR`, so something like
///     `[rules."clippy::too_many_lines"] threshold = 50` actually
///     reaches clippy
#[must_use = "diagnostics from clippy must be reported"]
///
/// `is_partial` (a git file scope) limits each run to the packages owning
/// `files`; a plain path scan lints the whole workspace.
pub fn lint_files(
    files: &[&SourceFile],
    config: &Config,
    is_partial: bool,
) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }
    let bindings = crate::rules::collect_clippy_bindings();
    let remap = remap::build_table(&bindings);
    let lint_args = lint_args(&bindings, config);

    let mut runs = Vec::new();
    let mut skipped = Vec::new();
    // Every root's clippy.toml is built before any clippy run starts.
    for (workspace, files_in_ws) in group_by_workspace(files) {
        let Some(root) = workspace else {
            skipped.extend(files_in_ws.iter().map(|f| f.path.display().to_string()));
            continue;
        };
        let clippy_conf = config_writer::materialize(config, &root, &files_in_ws)?;
        runs.push((root, files_in_ws, clippy_conf));
    }

    let mut diagnostics = Vec::new();
    for (root, files_in_ws, clippy_conf) in runs {
        let file_filter: FxHashSet<PathBuf> = files_in_ws
            .iter()
            .map(|f| canonicalize_or_self(&f.path))
            .collect();
        let packages = if is_partial {
            crate::runner_helpers::cargo_package_args(&root, &files_in_ws)
        } else {
            vec!["--workspace".to_owned()]
        };
        let run = |pass| invoke_clippy(&root, &packages, &lint_args, &clippy_conf.dir, pass);
        let mut output = run(Pass::Lint)?;
        ensure_config_accepted(&clippy_conf, &output.stdout)?;
        ensure_compiled(&root, &output)?;
        if denied_lints(&output.stdout).next().is_some() {
            ensure_compiled(&root, &run(Pass::Check)?)?;
            let capped = run(Pass::Capped)?;
            ensure_compiled(&root, &capped)?;
            ensure_packages_linted(&root, &files_in_ws, &output.stdout, &capped.stdout)?;
            output = capped;
        }
        diagnostics.extend(parse_clippy_jsonl(
            &output.stdout,
            &root,
            &file_filter,
            &remap,
        ));
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

/// The `-W`/`-A` flags passed after `--`, shared by lint and `--fix`:
/// clippy records them in each crate's fingerprint, so any difference
/// between the two modes re-lints the workspace when one follows the other.
pub(crate) fn lint_args(
    bindings: &[(&'static str, &'static RuleMeta, Severity)],
    config: &Config,
) -> Vec<String> {
    let mut args = build_lint_args(bindings);
    extend_with_user_enabled(&mut args, config);
    extend_with_user_disabled(&mut args, config);
    args
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
/// Every lint is added as `-W` (warn), never `-D` (deny): the comply driver
/// owns the final exit code, so clippy must not fail the run itself. The
/// registry only collects lints we *want* enabled, so there's no `-A`.
fn build_lint_args(bindings: &[(&'static str, &'static RuleMeta, Severity)]) -> Vec<String> {
    bindings
        .iter()
        .map(|(lint, _, _)| format!("-W{lint}"))
        .collect()
}

/// Group files by their workspace root. Files outside any Cargo workspace
/// land under the `None` key so the caller can warn the user.
pub(crate) fn group_by_workspace<'a>(
    files: &[&'a SourceFile],
) -> FxHashMap<Option<PathBuf>, Vec<&'a SourceFile>> {
    let mut out: FxHashMap<Option<PathBuf>, Vec<&'a SourceFile>> = FxHashMap::default();
    for f in files {
        let root = crate::runner_helpers::find_cargo_workspace_root(&f.path);
        out.entry(root).or_default().push(*f);
    }
    out
}

/// Build the `sqlx::query!` macros of `workspace` from its `.sqlx/` cache.
/// They reach the database at build time; the cache answers them without
/// one, as in a CI build. A `SQLX_OFFLINE` set by the caller wins.
pub(crate) fn use_sqlx_query_cache(cmd: &mut Command, workspace: &Path) {
    if workspace.join(".sqlx").is_dir() && std::env::var_os("SQLX_OFFLINE").is_none() {
        cmd.env("SQLX_OFFLINE", "true");
    }
}

/// One cargo run over a workspace.
///
/// A lint the code denies fails its crate.
/// Cargo then builds nothing that depends on it.
/// The last two passes judge what it hid.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    /// Clippy, with the lint levels the code sets.
    Lint,
    /// rustc alone, with the levels the code sets.
    Check,
    /// Clippy, with every denied lint lowered to a warning.
    /// It lowers rustc's own lints too, so [`Pass::Check`] judges those.
    Capped,
}

/// Spawn `cargo clippy` for one workspace and return the captured Output.
/// We pass `--quiet` to suppress cargo's progress noise on stderr, and
/// `--message-format=json` to get structured diagnostics on stdout.
///
/// `workspace` is the workspace root (resolved via `find_cargo_workspace_root`)
/// and `packages` the selection (`--workspace` or `-p` flags), so a root that
/// is itself a package still reaches the other members.
///
/// `clippy_conf_dir` holds comply's generated `clippy.toml` (`CLIPPY_CONF_DIR`),
/// which carries the keys of any project `clippy.toml` it accepted.
fn invoke_clippy(
    workspace: &Path,
    packages: &[String],
    lint_args: &[String],
    clippy_conf_dir: &Path,
    pass: Pass,
) -> Result<std::process::Output> {
    let manifest = workspace.join("Cargo.toml");
    let mut cmd = Command::new("cargo");
    cmd.arg(if pass == Pass::Check {
        "check"
    } else {
        "clippy"
    });
    cmd.args(packages);
    cmd.args([
        "--all-targets",
        // One failed crate must not hide an unrelated one.
        "--keep-going",
        "--message-format=json",
        "--quiet",
        // `is_unused_dependency_denied` reads cargo's errors as plain text.
        "--color=never",
        // `build.warnings` would hide the warnings, or fail on them.
        "--config",
        "build.warnings=\"warn\"",
        "--manifest-path",
    ]);
    cmd.arg(&manifest);
    if pass != Pass::Check {
        cmd.arg("--").args(lint_args);
    }
    if pass == Pass::Capped {
        cmd.args(["--cap-lints", "warn"]);
    }
    let (key, val) = config_writer::env_var_for_dir(clippy_conf_dir);
    cmd.env(key, val);
    use_sqlx_query_cache(&mut cmd, workspace);

    let output = cmd
        .output()
        .with_context(|| format!("failed to invoke `cargo clippy` for {}", manifest.display()))?;

    // The exit code is not read here: clippy exits non-zero both when lints
    // fired and when the build failed, which tells the two apart only in the
    // JSON stream. `ensure_config_accepted` and `ensure_compiled` read it.
    Ok(output)
}

/// Refuse a run whose generated `clippy.toml` clippy rejected (unknown key,
/// wrong type, duplicate): clippy then lints nothing. Each error names the
/// line of the file the key came from, never the generated file in the cache.
fn ensure_config_accepted(conf: &config_writer::ClippyConf, stdout: &[u8]) -> Result<()> {
    let generated = canonicalize_or_self(&conf.dir.join("clippy.toml"));
    let mut rejections: Vec<String> = Vec::new();
    for diag in compiler_messages(stdout).filter(|diag| diag.level == "error") {
        let Some(span) = diag.spans.iter().find(|span| {
            span.is_primary && canonicalize_or_self(Path::new(&span.file_name)) == generated
        }) else {
            continue;
        };
        let line = span.text.first().map_or("", |line| line.text.trim());
        let hint: String = diag
            .children
            .iter()
            .flat_map(|child| &child.spans)
            .filter_map(|span| span.suggested_replacement.as_deref())
            .map(|key| format!(" (did you mean `{key}`?)"))
            .collect();
        let rejection = format!(
            "{}: {}: `{line}`{hint}",
            conf.origin_of(span.line_start, span.column_start),
            diag.message
        );
        // Clippy repeats the error for every crate of the workspace.
        if !rejections.contains(&rejection) {
            rejections.push(rejection);
        }
    }
    if rejections.is_empty() {
        return Ok(());
    }
    Err(crate::toolchain::ToolchainError::Unusable {
        what: format!(
            "clippy rejected its configuration and linted no Rust file:\n{}",
            rejections.join("\n")
        ),
    }
    .into())
}

/// How many build errors the refusal quotes.
const QUOTED_BUILD_ERRORS: usize = 5;

/// Refuse a clippy run that could not build the workspace.
///
/// rustc's own errors carry no `clippy::` code, so `parse_clippy_jsonl`
/// drops them. A workspace that fails to compile therefore yielded zero
/// diagnostics and comply printed "all clear" on code clippy never saw.
/// Only a denied lint explains a failure without a build error.
/// Any other is refused with cargo's stderr.
/// Examples: a bad manifest, a build script that panics, a killed rustc.
fn ensure_compiled(workspace: &Path, output: &std::process::Output) -> Result<()> {
    let errors = build_errors(&output.stdout);
    let is_explained = output.status.success()
        || denied_lints(&output.stdout).next().is_some()
        || is_unused_dependency_denied(output.status.code(), &output.stderr);
    if errors.is_empty() && is_explained {
        return Ok(());
    }
    let detail = if errors.is_empty() {
        String::from_utf8_lossy(&output.stderr).trim().to_owned()
    } else {
        errors
            .into_iter()
            .take(QUOTED_BUILD_ERRORS)
            .collect::<Vec<_>>()
            .join("\n")
    };
    Err(crate::toolchain::ToolchainError::Unusable {
        what: format!(
            "cargo clippy could not compile {}; no Rust file was linted.\n{detail}",
            workspace.display()
        ),
    }
    .into())
}

/// Cargo's exit code for a build that failed.
const CARGO_FAILED: i32 = 101;

/// Whether cargo's unused-dependency lint alone failed the run.
/// Cargo judges a package once it is built.
/// Any other failure prints an `error` line of its own.
fn is_unused_dependency_denied(code: Option<i32>, stderr: &[u8]) -> bool {
    const FINDING: &str = "error: unused dependency `";
    const TALLY: &str = "error: could not finalize `";
    let stderr = String::from_utf8_lossy(stderr);
    let errors: Vec<&str> = stderr
        .lines()
        .filter(|line| line.starts_with("error"))
        .collect();
    code == Some(CARGO_FAILED)
        && errors.iter().any(|line| line.starts_with(FINDING))
        && errors
            .iter()
            .all(|line| line.starts_with(FINDING) || line.starts_with(TALLY))
}

/// Refuse a package of `files` that no clippy pass linted.
/// A denied lint stops the first pass.
/// The capped pass lints past it.
fn ensure_packages_linted(
    workspace: &Path,
    files: &[&SourceFile],
    first: &[u8],
    capped: &[u8],
) -> Result<()> {
    let mut linted = linted_manifests(first);
    linted.extend(linted_manifests(capped));
    let hidden: BTreeSet<String> = files
        .iter()
        .filter_map(|file| crate::runner_helpers::owning_manifest(workspace, &file.path))
        .filter(|(manifest, _)| !linted.contains(manifest))
        .map(|(_, package)| package)
        .collect();
    if hidden.is_empty() {
        return Ok(());
    }
    let quoted: Vec<String> = denied_lints(first)
        .filter_map(|envelope| envelope.message)
        .take(QUOTED_BUILD_ERRORS)
        .map(|lint| lint.rendered.unwrap_or(lint.message))
        .collect();
    Err(crate::toolchain::ToolchainError::Unusable {
        what: format!(
            "cargo clippy did not lint {} in {}, even with lints capped after this denied lint:\n{}",
            hidden.into_iter().collect::<Vec<_>>().join(", "),
            workspace.display(),
            quoted.join("\n")
        ),
    }
    .into())
}

/// The manifests of the packages clippy linted, one target at least.
/// A denied lint proves it; so does an artifact.
/// A build script proves nothing of the code it serves.
fn linted_manifests(stdout: &[u8]) -> FxHashSet<PathBuf> {
    envelopes(stdout)
        .filter(|envelope| envelope.reason == "compiler-artifact")
        .chain(denied_lints(stdout))
        .filter(|envelope| {
            !envelope
                .target
                .as_ref()
                .is_some_and(|target| target.kind.iter().any(|kind| kind == "custom-build"))
        })
        .filter_map(|envelope| envelope.manifest_path)
        .map(|manifest| canonicalize_or_self(Path::new(&manifest)))
        .collect()
}

/// rustc errors that stopped the build, as rustc rendered them.
/// A denied clippy lint is a finding, not a build error.
/// rustc's own panic has level `error: internal compiler error`.
fn build_errors(stdout: &[u8]) -> Vec<String> {
    compiler_messages(stdout)
        .filter(|diag| diag.level.starts_with("error"))
        .filter(|diag| !is_clippy_lint(diag))
        .filter(|diag| !diag.message.starts_with("aborting due to"))
        .map(|diag| diag.rendered.unwrap_or(diag.message))
        .collect()
}

/// The clippy lints whose level the code raised to an error.
fn denied_lints(stdout: &[u8]) -> impl Iterator<Item = CargoMessage> + '_ {
    envelopes(stdout).filter(|envelope| {
        envelope
            .message
            .as_ref()
            .is_some_and(|diag| diag.level == "error" && is_clippy_lint(diag))
    })
}

fn is_clippy_lint(diag: &RustcDiagnostic) -> bool {
    diag.code
        .as_ref()
        .is_some_and(|code| code.code.starts_with("clippy::"))
}

fn compiler_messages(stdout: &[u8]) -> impl Iterator<Item = RustcDiagnostic> + '_ {
    envelopes(stdout)
        .filter(|envelope| envelope.reason == "compiler-message")
        .filter_map(|envelope| envelope.message)
}

fn envelopes(stdout: &[u8]) -> impl Iterator<Item = CargoMessage> + '_ {
    BufReader::new(stdout)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<CargoMessage>(&line).ok())
}

/// Parse cargo's JSONL output stream and yield Diagnostic structs for
/// every primary span that lives in `file_filter`.
///
/// We surface every clippy lint, not the ones with an explicit
/// `Backend::Clippy` binding in comply's rule registry:
///
/// - If the lint code is in `remap`, the diagnostic is rewritten with
///   the comply rule_id and remediation severity (this is the "branded"
///   path — the user sees the comply name).
/// - Otherwise, if the lint code starts with `clippy::`, we pass it
///   through as-is. The rule_id stays as `clippy::needless_borrow`
///   (etc.) and the message is clippy's verbatim message. The user
///   doesn't lose the diagnostic because comply doesn't have a
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
    file_filter: &FxHashSet<PathBuf>,
    remap: &FxHashMap<String, &'static RuleMeta>,
) -> Vec<Diagnostic> {
    let reader = BufReader::new(stdout);
    let mut diagnostics = Vec::new();
    // `--all-targets` builds a crate as lib and as test: one finding each.
    let mut seen = FxHashSet::default();

    for line in reader.lines().map_while(Result::ok) {
        let Ok(envelope) = serde_json::from_str::<CargoMessage>(&line) else {
            continue;
        };
        if envelope.reason != "compiler-message" {
            continue;
        }
        let Some(diag) = envelope.message else {
            continue;
        };
        let Some(code) = diag.code.as_ref() else {
            continue;
        };

        // Decide how to surface this lint:
        //   - bound to a comply rule via remap → use the comply meta
        //   - any other clippy::* lint → pass through with raw code
        //   - anything else (rustc warnings without binding) → skip
        let mapped_meta = remap.get(&code.code).copied();
        if mapped_meta.is_none() && !code.code.starts_with("clippy::") {
            continue;
        }

        let Some(span) = diag.spans.iter().find(|s| s.is_primary) else {
            continue;
        };

        let span_path = span_path(span, workspace_root);
        if !file_filter.contains(&span_path) {
            continue;
        }
        let location = (
            span_path.clone(),
            span.line_start,
            span.column_start,
            code.code.clone(),
        );
        if !seen.insert(location) {
            continue;
        }

        let severity = Severity::Error;

        let rule_id = match mapped_meta {
            Some(meta) => std::borrow::Cow::Borrowed(meta.id),
            None => std::borrow::Cow::Owned(code.code.clone()),
        };

        diagnostics.push(Diagnostic {
            path: std::sync::Arc::from(span_path.as_path()),
            line: span.line_start.max(1),
            column: span.column_start.max(1),
            rule_id,
            message: with_disallowed_reason(diag.message, &diag.children, &code.code),
            severity,
            span: None,
        });
    }

    diagnostics
}

/// The file of `span`, in the form `file_filter` holds.
/// Cargo names it relative to the workspace root, or in full.
fn span_path(span: &RustcSpan, workspace_root: &Path) -> PathBuf {
    canonicalize_or_self(&workspace_root.join(&span.file_name))
}

/// Append the `reason` of a `disallowed-*` entry to clippy's message.
/// Provenance notes (`#[warn(clippy::…)]` on by default) name the lint.
fn with_disallowed_reason(message: String, children: &[RustcDiagnostic], code: &str) -> String {
    if !code.starts_with("clippy::disallowed_") {
        return message;
    }
    let reasons: Vec<&str> = children
        .iter()
        .filter(|c| c.level == "note" && !is_lint_level_note(&c.message))
        .map(|c| c.message.as_str())
        .collect();
    if reasons.is_empty() {
        return message;
    }
    format!("{message}: {}", reasons.join("; "))
}

/// Whether a note is rustc's lint-level provenance.
fn is_lint_level_note(message: &str) -> bool {
    [
        "`#[",
        "`-",
        "requested on the command line",
        "the lint level is defined",
    ]
    .iter()
    .any(|prefix| message.starts_with(prefix))
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

    #[test]
    fn only_a_denied_unused_dependency_explains_a_cargo_failure() {
        let lint = "error: unused dependency `idle`\n  --> Cargo.toml:9:1\n   |\n   \
                    = note: `cargo::unused_dependencies` is set to `deny` in `[lints]`\n\
                    error: could not finalize `app` (manifest) due to 1 previous error\n";
        assert!(is_unused_dependency_denied(Some(101), lint.as_bytes()));
        // A killed cargo judged nothing for sure.
        assert!(!is_unused_dependency_denied(None, lint.as_bytes()));
        for other in [
            "error: could not compile `b` (lib)\n",
            "error: failed to run custom build command for `b v0.1.0`\n",
            "error[E0583]: file not found for module `gone`\n",
        ] {
            let stderr = format!("{lint}{other}");
            assert!(
                !is_unused_dependency_denied(Some(101), stderr.as_bytes()),
                "{other}"
            );
        }
        assert!(!is_unused_dependency_denied(Some(101), b""));
        assert!(!is_unused_dependency_denied(
            Some(101),
            b"error: could not finalize `app` (manifest) due to 1 previous error\n"
        ));
    }

    #[test]
    fn build_errors_keeps_rustc_errors_and_leaves_denied_clippy_lints() {
        let json = br#"{"reason":"compiler-message","message":{"message":"mismatched types","code":{"code":"E0308"},"level":"error","rendered":"error[E0308]: mismatched types","spans":[]}}
{"reason":"compiler-message","message":{"message":"error communicating with database","code":null,"level":"error","spans":[]}}
{"reason":"compiler-message","message":{"message":"used unwrap","code":{"code":"clippy::unwrap_used"},"level":"error","spans":[]}}
{"reason":"compiler-message","message":{"message":"no errors encountered","code":null,"level":"error: internal compiler error","spans":[]}}
{"reason":"compiler-message","message":{"message":"aborting due to 2 previous errors","code":null,"level":"error","spans":[]}}
{"reason":"compiler-message","message":{"message":"unused variable","code":{"code":"unused_variables"},"level":"warning","spans":[]}}
{"reason":"build-finished","success":false}"#;
        assert_eq!(
            build_errors(json),
            vec![
                "error[E0308]: mismatched types".to_owned(),
                "error communicating with database".to_owned(),
                "no errors encountered".to_owned()
            ]
        );
    }

    /// `ensure_packages_linted` over `files` of members `a` and `b`.
    /// The first pass stops at a denied lint of `a`.
    fn judge_packages(files: &[&str], capped: &[&str]) -> Result<()> {
        let dir = tempfile::TempDir::new().unwrap();
        let root = canonicalize_or_self(dir.path());
        let manifest = |name: &str| root.join(name).join("Cargo.toml");
        for name in ["a", "b"] {
            std::fs::create_dir_all(root.join(name).join("src")).unwrap();
            let package = format!("[package]\nname = \"{name}\"\n");
            std::fs::write(manifest(name), package).unwrap();
            std::fs::write(root.join(name).join("src/lib.rs"), "").unwrap();
        }
        let first = format!(
            r#"{{"reason":"compiler-message","manifest_path":"{}","message":{{"message":"x","code":{{"code":"clippy::absurd_extreme_comparisons"}},"level":"error","rendered":"error: x < 0","spans":[]}}}}"#,
            manifest("a").display()
        );
        let capped: Vec<String> = capped
            .iter()
            .map(|name| {
                format!(
                    r#"{{"reason":"compiler-artifact","manifest_path":"{}","target":{{"kind":["lib"]}}}}"#,
                    manifest(name).display()
                )
            })
            .collect();
        let files: Vec<SourceFile> = files
            .iter()
            .map(|name| SourceFile {
                path: root.join(name).join("src/lib.rs"),
                language: crate::files::Language::Rust,
            })
            .collect();
        let files: Vec<&SourceFile> = files.iter().collect();
        ensure_packages_linted(
            &root,
            &files,
            first.as_bytes(),
            capped.join("\n").as_bytes(),
        )
    }

    #[test]
    fn a_package_the_capped_pass_lints_is_judged() {
        assert!(judge_packages(&["b"], &["a", "b"]).is_ok());
        // The denied lint proves `a` was linted.
        assert!(judge_packages(&["a"], &[]).is_ok());
    }

    #[test]
    fn a_package_no_pass_lints_is_refused() {
        let refusal = judge_packages(&["a", "b"], &["a"]).unwrap_err().to_string();
        assert!(refusal.contains("did not lint b in"), "{refusal}");
        assert!(refusal.contains("x < 0"), "{refusal}");
    }

    #[test]
    fn parse_clippy_jsonl_extracts_diagnostics_for_filtered_files() {
        const META: RuleMeta = RuleMeta {
            id: "rust-no-unwrap",
            description: "no unwrap",
            remediation: "use ?",
            severity: Severity::Error,
            doc_url: None,
            categories: &[],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        };
        let mut remap: FxHashMap<String, &'static RuleMeta> = FxHashMap::default();
        remap.insert("clippy::unwrap_used".to_string(), &META);

        let json = br#"{"reason":"compiler-message","message":{"message":"used unwrap","code":{"code":"clippy::unwrap_used"},"level":"warning","spans":[{"file_name":"/abs/src/main.rs","line_start":10,"column_start":5,"is_primary":true}]}}
{"reason":"build-finished","success":true}"#;

        let mut filter = FxHashSet::default();
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
            categories: &[],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        };
        let mut remap: FxHashMap<String, &'static RuleMeta> = FxHashMap::default();
        remap.insert("clippy::unwrap_used".to_string(), &META);

        let json = br#"{"reason":"compiler-message","message":{"message":"used unwrap","code":{"code":"clippy::unwrap_used"},"level":"warning","spans":[{"file_name":"/abs/other.rs","line_start":1,"column_start":1,"is_primary":true}]}}"#;

        let mut filter = FxHashSet::default();
        filter.insert(PathBuf::from("/abs/wanted.rs"));

        let diagnostics = parse_clippy_jsonl(json, Path::new("/abs"), &filter, &remap);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parse_clippy_jsonl_skips_non_compiler_messages() {
        let remap: FxHashMap<String, &'static RuleMeta> = FxHashMap::default();
        let filter: FxHashSet<PathBuf> = FxHashSet::default();
        let json = br#"{"reason":"build-finished","success":true}
{"reason":"compiler-artifact","package_id":"x"}"#;
        let diagnostics = parse_clippy_jsonl(json, Path::new("/abs"), &filter, &remap);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parse_clippy_jsonl_passes_through_unbound_clippy_lints() {
        // No remap entry — but the lint code starts with `clippy::`,
        // so we keep the diagnostic with its raw rule_id.
        let remap: FxHashMap<String, &'static RuleMeta> = FxHashMap::default();
        let json = br#"{"reason":"compiler-message","message":{"message":"needless borrow","code":{"code":"clippy::needless_borrow"},"level":"warning","spans":[{"file_name":"/abs/src/main.rs","line_start":4,"column_start":2,"is_primary":true}]}}"#;
        let mut filter = FxHashSet::default();
        filter.insert(PathBuf::from("/abs/src/main.rs"));

        let diagnostics = parse_clippy_jsonl(json, Path::new("/abs"), &filter, &remap);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule_id, "clippy::needless_borrow");
        assert_eq!(diagnostics[0].message, "needless borrow");
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn parse_clippy_jsonl_skips_unbound_rustc_warnings() {
        // `dead_code` is a rustc lint, not clippy, and it has no
        // explicit binding — comply leaves it to the compiler.
        let remap: FxHashMap<String, &'static RuleMeta> = FxHashMap::default();
        let json = br#"{"reason":"compiler-message","message":{"message":"function `foo` is never used","code":{"code":"dead_code"},"level":"warning","spans":[{"file_name":"/abs/src/main.rs","line_start":1,"column_start":1,"is_primary":true}]}}"#;
        let mut filter = FxHashSet::default();
        filter.insert(PathBuf::from("/abs/src/main.rs"));

        let diagnostics = parse_clippy_jsonl(json, Path::new("/abs"), &filter, &remap);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parse_clippy_jsonl_appends_disallowed_reason() {
        let remap: FxHashMap<String, &'static RuleMeta> = FxHashMap::default();
        let json = br#"{"reason":"compiler-message","message":{"message":"use of a disallowed method `std::env::var`","code":{"code":"clippy::disallowed_methods"},"level":"warning","spans":[{"file_name":"/abs/src/main.rs","line_start":3,"column_start":5,"is_primary":true}],"children":[{"message":"read Config","level":"note","spans":[],"children":[]},{"message":"for further information visit https://rust-lang.github.io/rust-clippy","level":"help","spans":[],"children":[]},{"message":"`#[warn(clippy::disallowed_methods)]` on by default","level":"note","spans":[],"children":[]}]}}"#;
        let mut filter = FxHashSet::default();
        filter.insert(PathBuf::from("/abs/src/main.rs"));

        let diagnostics = parse_clippy_jsonl(json, Path::new("/abs"), &filter, &remap);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "use of a disallowed method `std::env::var`: read Config"
        );
    }

    #[test]
    fn parse_clippy_jsonl_reports_a_span_once_across_targets() {
        let remap: FxHashMap<String, &'static RuleMeta> = FxHashMap::default();
        let line = r#"{"reason":"compiler-message","message":{"message":"length comparison to zero","code":{"code":"clippy::len_zero"},"level":"warning","spans":[{"file_name":"/abs/src/lib.rs","line_start":2,"column_start":8,"is_primary":true}]}}"#;
        let json = format!("{line}\n{line}\n");
        let mut filter = FxHashSet::default();
        filter.insert(PathBuf::from("/abs/src/lib.rs"));

        let diagnostics = parse_clippy_jsonl(json.as_bytes(), Path::new("/abs"), &filter, &remap);
        assert_eq!(diagnostics.len(), 1);
    }
}
