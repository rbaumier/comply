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
//!            --all-targets --message-format=json --quiet \
//!            --manifest-path <workspace>/Cargo.toml \
//!            -- -W clippy::lint1 -W clippy::lint2 ...
//!
//!    rustc's `unused_crate_dependencies` lint is forced on as well.
//!    Its verdicts become an [`ExternUsage`] for `rust-unused-dep`.
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
pub(crate) mod config_writer;
mod remap;
mod schema;

use anyhow::{Context, Result};
use rustc_hash::{FxHashMap, FxHashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::config::Config;
use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;
use crate::rules::meta::RuleMeta;
use schema::{CargoMessage, RustcDiagnostic};

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
/// The [`ExternUsage`] of every crate clippy compiled comes along.
///
/// `config` is the resolved per-project comply.toml. We use it to:
///   - add `-W clippy::xxx` for any lint flagged `enabled = true`
///     in `[rules."clippy::xxx"]` (most clippy lints default to
///     `allow`, so this is how the user opts in to extra ones)
///   - materialize a `clippy.toml` from per-lint thresholds and
///     `[clippy]`, and point cargo at it via `CLIPPY_CONF_DIR`, so something like
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
) -> Result<(Vec<Diagnostic>, ExternUsage)> {
    let mut usage = ExternUsage::default();
    if files.is_empty() {
        return Ok((vec![], usage));
    }
    let bindings = crate::rules::collect_clippy_bindings();
    let remap = remap::build_table(&bindings);
    let lint_args = lint_args(&bindings, config);
    let clippy_conf_dir = config_writer::materialize(config)?;

    let workspaces = group_by_workspace(files);
    // Every root is checked before any clippy run starts.
    for root in workspaces.keys().flatten() {
        config_writer::reject_project_clippy_toml(root)?;
    }
    let mut diagnostics = Vec::new();
    let mut skipped = Vec::new();

    for (workspace, files_in_ws) in workspaces {
        match workspace {
            Some(root) => {
                let file_filter: FxHashSet<PathBuf> = files_in_ws
                    .iter()
                    .map(|f| canonicalize_or_self(&f.path))
                    .collect();
                let packages = if is_partial {
                    crate::runner_helpers::cargo_package_args(&root, &files_in_ws)
                } else {
                    vec!["--workspace".to_owned()]
                };
                let output = invoke_clippy(&root, &packages, &lint_args, &clippy_conf_dir)?;
                usage.record(&output.stdout, &root);
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

    Ok((diagnostics, usage))
}

/// Forces rustc to name each dependency a crate target leaves unused.
/// Forced, so a crate's own `allow` or `deny` cannot change it.
/// The lint carries no fix, so `--fix` never edits code for it.
const UNUSED_EXTERNS_LINT: &str = "--force-warn=unused_crate_dependencies";

/// The `-W`/`-A` flags passed after `--`, shared by lint and `--fix`:
/// clippy records them in each crate's fingerprint, so any difference
/// between the two modes re-lints the workspace when one follows the other.
pub(crate) fn lint_args(
    bindings: &[(&'static str, &'static RuleMeta, Severity)],
    config: &Config,
) -> Vec<String> {
    let mut args = build_lint_args(bindings);
    args.push(UNUSED_EXTERNS_LINT.to_owned());
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

/// Spawn `cargo clippy` for one workspace and return the captured Output.
/// We pass `--quiet` to suppress cargo's progress noise on stderr, and
/// `--message-format=json` to get structured diagnostics on stdout.
///
/// `workspace` is the workspace root (resolved via `find_cargo_workspace_root`)
/// and `packages` the selection (`--workspace` or `-p` flags), so a root that
/// is itself a package still reaches the other members.
///
/// `clippy_conf_dir` holds comply's generated `clippy.toml` (`CLIPPY_CONF_DIR`).
/// `lint_files` refused any project `clippy.toml` beforehand.
fn invoke_clippy(
    workspace: &Path,
    packages: &[String],
    lint_args: &[String],
    clippy_conf_dir: &Path,
) -> Result<std::process::Output> {
    let manifest = workspace.join("Cargo.toml");
    let mut cmd = Command::new("cargo");
    cmd.arg("clippy").args(packages);
    cmd.args([
        "--all-targets",
        "--message-format=json",
        "--quiet",
        "--manifest-path",
    ]);
    cmd.arg(&manifest);
    cmd.arg("--");
    for arg in lint_args {
        cmd.arg(arg);
    }
    let (key, val) = config_writer::env_var_for_dir(clippy_conf_dir);
    cmd.env(key, val);

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

/// Which dependencies rustc left unused, per compiled workspace target.
///
/// rustc judges after macro expansion.
/// It sees what a derive emits and what `include!` loads from `OUT_DIR`.
/// cargo-shear reads the sources only, so it misses both.
#[derive(Debug, Default)]
pub struct ExternUsage {
    packages: FxHashMap<PathBuf, PackageExterns>,
}

/// rustc's verdicts on the targets of one package.
#[derive(Debug, Default)]
pub struct PackageExterns {
    /// Targets cargo reported built, build scripts excluded.
    pub compiled: FxHashSet<(Vec<String>, String)>,
    /// Externs rustc reported unused, per target.
    pub unused: FxHashMap<(Vec<String>, String), FxHashSet<String>>,
    /// rustc reported an unused extern without naming it readably.
    pub unreadable: bool,
}

impl ExternUsage {
    /// rustc's verdicts on the package of `manifest`, if clippy built it.
    pub fn package(&self, manifest: &Path) -> Option<&PackageExterns> {
        self.packages.get(&canonicalize_or_self(manifest))
    }

    /// Read the artifacts and unused-extern warnings of one clippy run.
    /// Crates outside `workspace` are dependencies, so they are left out.
    fn record(&mut self, stdout: &[u8], workspace: &Path) {
        let root = canonicalize_or_self(workspace);
        for envelope in envelopes(stdout) {
            let (Some(manifest), Some(target)) = (envelope.manifest_path, envelope.target) else {
                continue;
            };
            if target.kind.iter().any(|kind| kind == "custom-build") {
                continue;
            }
            let manifest = canonicalize_or_self(Path::new(&manifest));
            if !manifest.starts_with(&root) {
                continue;
            }
            let package = self.packages.entry(manifest).or_default();
            let key = (target.kind, target.name);
            match (envelope.reason.as_str(), envelope.message) {
                ("compiler-artifact", _) => {
                    package.compiled.insert(key);
                }
                ("compiler-message", Some(diag)) if is_unused_extern(&diag) => {
                    match diag.message.split('`').nth(1) {
                        Some(name) if !name.is_empty() => {
                            package
                                .unused
                                .entry(key)
                                .or_default()
                                .insert(name.to_owned());
                        }
                        _ => package.unreadable = true,
                    }
                }
                _ => {}
            }
        }
    }
}

/// An `unused_crate_dependencies` warning.
/// rustc words it ``extern crate `name` is unused in crate `krate` ``.
fn is_unused_extern(diag: &RustcDiagnostic) -> bool {
    diag.code
        .as_ref()
        .is_some_and(|code| code.code == "unused_crate_dependencies")
}

/// Every cargo message of a `--message-format=json` stream.
/// Lines that are not one are skipped.
fn envelopes(stdout: &[u8]) -> impl Iterator<Item = CargoMessage> + '_ {
    BufReader::new(stdout)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<CargoMessage>(&line).ok())
}

/// Canonicalize the path if possible, otherwise return it as-is.
/// We need canonical paths so that filter matches work even when the
/// user passed a relative path on the CLI but cargo emits absolute.
pub(crate) fn canonicalize_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extern_usage_reads_artifacts_and_unused_extern_warnings() {
        let ws = std::env::temp_dir().join("comply-extern-usage");
        let app = ws.join("app/Cargo.toml");
        let line = |reason: &str, kind: &str, message: &str| {
            format!(
                r#"{{"reason":"{reason}","manifest_path":"{}","target":{{"name":"app","kind":["{kind}"]}}{message}}}"#,
                app.display()
            )
        };
        let unused = |text: &str| {
            format!(
                r#","message":{{"message":"{text}","level":"warning","code":{{"code":"unused_crate_dependencies"}}}}"#
            )
        };
        let stream = [
            line("compiler-message", "lib", &unused("extern crate `idle` is unused in crate `app`")),
            line("compiler-artifact", "lib", ""),
            line("compiler-artifact", "custom-build", ""),
            r#"{"reason":"compiler-artifact","manifest_path":"/registry/dep/Cargo.toml","target":{"name":"dep","kind":["lib"]}}"#.to_owned(),
        ]
        .join("\n");
        let mut usage = ExternUsage::default();
        usage.record(stream.as_bytes(), &ws);

        let package = usage.package(&app).expect("app is a workspace crate");
        let lib = (vec!["lib".to_owned()], "app".to_owned());
        assert_eq!(
            package.compiled.len(),
            1,
            "a build script is no target to prove with"
        );
        assert!(package.compiled.contains(&lib));
        assert!(package.unused[&lib].contains("idle"));
        assert!(!package.unreadable);
        assert!(
            usage
                .package(Path::new("/registry/dep/Cargo.toml"))
                .is_none()
        );

        let mut garbled = ExternUsage::default();
        garbled.record(
            line("compiler-message", "lib", &unused("unused crate")).as_bytes(),
            &ws,
        );
        assert!(garbled.package(&app).unwrap().unreadable);
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
