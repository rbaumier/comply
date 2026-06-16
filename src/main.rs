//! comply — your code will comply.
//!
//! Enforces coding-standards rules via syntactic analysis. Dispatches to oxlint
//! for TS/JS linting, applies custom tree-sitter rules in-process, and unifies
//! all output into ESLint-like format with remediation messages.
//!
//! Pipeline overview:
//! 1. Parse CLI args → ScanMode (which files to lint).
//! 2. Discover files via filesystem walk or git diff.
//! 3. For TS/JS files: invoke oxlint subprocess (if installed) AND apply
//!    custom tree-sitter rules. The two passes are complementary —
//!    oxlint catches type/style issues, custom rules catch architecture issues.
//! 4. For Rust files: apply custom rules only (clippy integration is v2).
//! 5. Apply comply-ignore suppressions across every discovered file.
//! 6. Format diagnostics, print, exit 0/1/2.

mod cargo_modules;
mod cargo_shear;
mod catalog;
mod changed_lines;
mod cli;
mod clippy;
mod clone_detection;
mod comment_dup_detection;
mod config;
mod diagnostic;
mod engine;
mod explain;
mod files;
mod fix;
mod frameworks;
mod icu;
mod ignore_comments;
mod list;
mod lsp;
mod output;
mod oxc_helpers;
mod oxlint;
mod oxlint_config;
mod parsing;
mod project;
mod rules;
mod runner_helpers;
mod tui;
mod typeaware;

use std::io::IsTerminal;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command, ConfigAction, ScanMode};
use config::Config;
use diagnostic::Diagnostic;
use files::{Language, SourceFile};

/// The whole pipeline is allocation-heavy and massively parallel (one
/// arena + AST + diagnostic buffers per file across every rayon worker).
/// mimalloc's per-thread heaps cut cross-thread allocator contention that
/// the system allocator serializes on, with no behavioural change.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const RAYON_WORKER_STACK_SIZE_BYTES: usize = 32 * 1024 * 1024;

fn main() -> ExitCode {
    rayon::ThreadPoolBuilder::new()
        .stack_size(RAYON_WORKER_STACK_SIZE_BYTES)
        .build_global()
        .ok();
    match run() {
        Ok(true) => ExitCode::from(1),  // violations found
        Ok(false) => ExitCode::from(0), // clean
        Err(e) => {
            eprintln!(
                "comply: crashed unexpectedly: {e:#}\n\
                 Re-run with RUST_BACKTRACE=1 and report at https://github.com/rbaumier/comply/issues"
            );
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::{Language, SourceFile};

    #[test]
    fn rayon_worker_stack_size_covers_deep_project_walks() {
        assert!(RAYON_WORKER_STACK_SIZE_BYTES >= 32 * 1024 * 1024);
    }

    fn write_ts(dir: &tempfile::TempDir, name: &str, content: &str) -> SourceFile {
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        SourceFile { path, language: Language::TypeScript }
    }

    #[test]
    fn rules_subcommand_recognizes_all_rule_families() {
        // A real engine rule — whichever the registry lists first.
        assert!(is_known_rule_id(rules::all_rule_defs()[0].meta.id));
        // In-process cross-file detectors (absent from `all_rule_defs`).
        assert!(is_known_rule_id(clone_detection::RULE_ID));
        assert!(is_known_rule_id(comment_dup_detection::RULE_ID));
        // Cargo subprocess detectors (also absent from `all_rule_defs`).
        assert!(is_known_rule_id(cargo_modules::RULE_ID));
        assert!(is_known_rule_id(cargo_shear::RULE_ID));
        // Unknown IDs are still rejected so the subcommand keeps erroring on typos.
        assert!(!is_known_rule_id("not-a-real-rule"));
    }

    #[test]
    fn subprocess_routing_special_cases_standalone_rules() {
        // Cargo-backed rules live outside the engine and need a subprocess,
        // so `comply rules` must route them through the full pipeline.
        assert!(rule_requires_subprocess(cargo_modules::RULE_ID));
        assert!(rule_requires_subprocess(cargo_shear::RULE_ID));
        // In-process cross-file detectors run on the fast path instead.
        assert!(!rule_requires_subprocess(clone_detection::RULE_ID));
        assert!(!rule_requires_subprocess(comment_dup_detection::RULE_ID));
    }

    #[test]
    fn run_cross_file_rules_dispatches_only_requested_detectors() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_ts(
            &dir,
            "a.ts",
            "// Defaults derived from the canonical schema definition and kept in sync \
             with the database migration layer manually.\nexport const a = 1;\n",
        );
        let b = write_ts(
            &dir,
            "b.ts",
            "// Defaults derived from the canonical schema definition and kept in sync \
             with the database migration layer by hand.\nexport const b = 2;\n",
        );
        let cfg = Config::default();
        let requested = vec![comment_dup_detection::RULE_ID.to_string()];

        let diags = run_cross_file_rules(&requested, &[&a, &b], &cfg);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, comment_dup_detection::RULE_ID);

        // A detector not named in the filter is not dispatched.
        let unrelated = vec!["boolean-naming".to_string()];
        assert!(run_cross_file_rules(&unrelated, &[&a, &b], &cfg).is_empty());

        // Cross-file detectors need at least two files.
        assert!(run_cross_file_rules(&requested, &[&a], &cfg).is_empty());
    }
}

/// Dispatch on the top-level subcommand. Default = lint.
fn run() -> Result<bool> {
    let cli = Cli::parse();
    if cli.tui && cli.command.is_some() {
        eprintln!("warning: --tui is ignored when a subcommand is specified");
    }
    match cli.command {
        Some(Command::Explain { ref rule_id }) => {
            explain::run(rule_id)?;
            Ok(false)
        }
        Some(Command::List { should_emit_json }) => {
            list::run(should_emit_json)?;
            Ok(false)
        }
        Some(Command::Catalog { should_emit_json }) => {
            catalog::run(should_emit_json)?;
            Ok(false)
        }
        Some(Command::Rules {
            ref rule_ids,
            should_emit_json,
            ref path,
        }) => {
            let filter: Vec<String> = rule_ids
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if filter.is_empty() {
                eprintln!("comply: no rule IDs provided");
                return Ok(true);
            }
            let unknown: Vec<&str> = filter
                .iter()
                .filter(|id| !is_known_rule_id(id))
                .map(String::as_str)
                .collect();
            if !unknown.is_empty() {
                eprintln!(
                    "comply: unknown rule(s): {}\n\
                     Run `comply list` to see all available rule IDs.",
                    unknown.join(", ")
                );
                return Ok(true);
            }
            lint_with_rules(
                &filter,
                path.clone()
                    .unwrap_or_else(|| std::path::PathBuf::from(".")),
                should_emit_json,
            )
        }
        Some(Command::Config { ref action }) => {
            run_config_action(action)?;
            Ok(false)
        }
        Some(Command::Lsp) => {
            // Spin up a small tokio runtime for the LSP server.
            // Comply itself is sync; we don't pay the runtime cost
            // unless the user actually starts the LSP.
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|e| anyhow::anyhow!("failed to start tokio runtime: {e}"))?;
            runtime.block_on(lsp::run());
            Ok(false)
        }
        None => lint_project(&cli),
    }
}

/// Handle `comply config init` and `comply config print`.
fn run_config_action(action: &ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Init { force } => {
            let cwd = std::env::current_dir()?;
            let target = cwd.join(config::CONFIG_FILE_NAME);
            if target.exists() && !force {
                eprintln!(
                    "comply: {} already exists — pass --force to overwrite",
                    target.display()
                );
                return Ok(());
            }
            std::fs::write(&target, Config::print_default_toml())?;
            println!("comply: wrote {}", target.display());
        }
        ConfigAction::Print => {
            print!("{}", Config::print_default_toml());
        }
    }
    Ok(())
}

/// Per-phase wall-clock collected when `--timings` is set.
///
/// Dev-only instrumentation — lets us see exactly which subprocess or
/// in-process phase dominates a given run before touching optimization.
/// All fields default to `Duration::ZERO` so unused phases (e.g. no Rust
/// files → clippy unused) render as `0ms`.
#[derive(Default, Debug)]
struct Timings {
    discovery: Duration,
    config: Duration,
    fix: Duration,
    oxlint: Duration,
    engine_ts: Duration,
    type_aware: Duration,
    clippy: Duration,
    cargo_shear: Duration,
    cargo_modules: Duration,
    engine_rs: Duration,
    engine_vue: Duration,
    clones: Duration,
    post: Duration,
    total: Duration,
}

fn fmt_ms(d: Duration) -> String {
    format!("{:>7.1}ms", d.as_secs_f64() * 1000.0)
}

fn print_timings(t: &Timings) {
    eprintln!("comply: timings breakdown");
    eprintln!("  discovery     {}", fmt_ms(t.discovery));
    eprintln!("  config        {}", fmt_ms(t.config));
    if t.fix > Duration::ZERO {
        eprintln!("  fix           {}", fmt_ms(t.fix));
    }
    eprintln!("  -- typescript --");
    eprintln!("  oxlint        {}", fmt_ms(t.oxlint));
    eprintln!("  engine (ts)   {}", fmt_ms(t.engine_ts));
    if t.type_aware > Duration::ZERO {
        eprintln!("  type-aware    {}", fmt_ms(t.type_aware));
    }
    eprintln!("  -- rust --");
    eprintln!("  clippy        {}", fmt_ms(t.clippy));
    eprintln!("  cargo-shear   {}", fmt_ms(t.cargo_shear));
    eprintln!("  cargo-modules {}", fmt_ms(t.cargo_modules));
    eprintln!("  engine (rs)   {}", fmt_ms(t.engine_rs));
    eprintln!("  -- vue --");
    eprintln!("  engine (vue)  {}", fmt_ms(t.engine_vue));
    eprintln!("  -- cross-file --");
    eprintln!("  clones        {}", fmt_ms(t.clones));
    eprintln!("  post-filter   {}", fmt_ms(t.post));
    eprintln!("  -----");
    eprintln!("  TOTAL         {}", fmt_ms(t.total));
}

/// Cross-file detectors that run in-process outside the rule engine, so they
/// are absent from `all_rule_defs()`. `comply rules` dispatches these directly
/// when their ID is requested.
const CROSS_FILE_RULE_IDS: &[&str] = &[clone_detection::RULE_ID, comment_dup_detection::RULE_ID];

/// Cargo-backed subprocess detectors, also absent from `all_rule_defs()`.
/// Producing them requires shelling out to cargo, so a `comply rules` filter
/// naming one routes through the full `collect_all_diagnostics` pipeline.
const CARGO_SUBPROCESS_RULE_IDS: &[&str] = &[cargo_modules::RULE_ID, cargo_shear::RULE_ID];

/// Whether `id` names a rule comply can emit — an engine rule, an in-process
/// cross-file detector, or a subprocess-backed rule.
fn is_known_rule_id(id: &str) -> bool {
    rules::all_rule_defs().iter().any(|r| r.meta.id == id)
        || CROSS_FILE_RULE_IDS.contains(&id)
        || CARGO_SUBPROCESS_RULE_IDS.contains(&id)
}

/// Whether producing `id` requires an external subprocess (oxlint, clippy,
/// tsgolint, tsc, cargo-shear, cargo-modules). The in-process engine returns
/// nothing for these backends, so `comply rules` must run the full pipeline.
fn rule_requires_subprocess(id: &str) -> bool {
    use crate::rules::backend::Backend;
    CARGO_SUBPROCESS_RULE_IDS.contains(&id)
        || rules::all_rule_defs().iter().any(|r| {
            r.meta.id == id
                && r.backends.iter().any(|(_, b)| {
                    matches!(
                        b,
                        Backend::Oxlint { .. }
                            | Backend::Clippy { .. }
                            | Backend::Tsc { .. }
                            | Backend::Tsgolint { .. }
                            | Backend::TypeAware
                    )
                })
        })
}

/// Whether `id` is enforced through the type-aware sidecar (tsgolint / the
/// type-aware backend), which the full pipeline only runs when asked.
fn rule_requires_type_aware(id: &str) -> bool {
    use crate::rules::backend::Backend;
    rules::all_rule_defs().iter().any(|r| {
        r.meta.id == id
            && r.backends
                .iter()
                .any(|(_, b)| matches!(b, Backend::Tsgolint { .. } | Backend::TypeAware))
    })
}

/// Run the in-process cross-file detectors named in `filter`. They need the
/// full file set (≥2 files) and live outside the engine, so they are
/// dispatched here rather than through `engine::lint_files_with_project`.
fn run_cross_file_rules(
    filter: &[String],
    files: &[&SourceFile],
    config: &Config,
) -> Vec<Diagnostic> {
    let wants = |id: &str| filter.iter().any(|f| f.as_str() == id);
    let mut out = Vec::new();
    if files.len() >= 2 {
        if wants(clone_detection::RULE_ID) {
            out.extend(clone_detection::lint_files(files));
        }
        if wants(comment_dup_detection::RULE_ID) {
            out.extend(comment_dup_detection::lint_files(files, config));
        }
    }
    out
}

/// Lint only the rules whose IDs are in `filter`.
///
/// In-process rules (engine rules plus the `no-clones` / `no-duplicate-comments`
/// cross-file detectors) run through the filtered engine and a direct
/// cross-file dispatch, keeping per-rule runs fast. When the filter names a
/// subprocess-backed rule (oxlint, clippy, tsgolint, cargo-shear,
/// cargo-modules) the full pipeline runs and the result is narrowed to the
/// requested rules afterwards.
fn lint_with_rules(
    filter: &[String],
    path: std::path::PathBuf,
    should_emit_json: bool,
) -> Result<bool> {
    let mode = cli::ScanMode::All(path);
    let discovered = files::discover(&mode)?;
    if discovered.is_empty() {
        if should_emit_json {
            println!("[]");
        } else {
            println!("comply: no files to lint");
        }
        return Ok(false);
    }

    let config_anchor = discovered
        .first()
        .and_then(|f| f.path.parent())
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let config = Config::load_from(&config_anchor)?;

    let (diagnostics, clean_files) = if filter.iter().any(|id| rule_requires_subprocess(id)) {
        let type_aware = filter.iter().any(|id| rule_requires_type_aware(id));
        let mut timings = Timings::default();
        collect_all_diagnostics(&discovered, &config, &mut timings, false, type_aware, false)?
    } else {
        let all_refs: Vec<&SourceFile> = discovered.iter().collect();
        let project = std::sync::Arc::new(crate::project::ProjectCtx::load(&all_refs, &config));
        let mut diags =
            engine::lint_files_with_project(&all_refs, &config, &project, Some(filter))?;
        diags.extend(run_cross_file_rules(filter, &all_refs, &config));
        (diags, project.clean_files_snapshot())
    };

    // The full-pipeline branch emits every rule; narrow to what was requested.
    let requested: Vec<Diagnostic> = diagnostics
        .into_iter()
        .filter(|d| filter.iter().any(|id| id.as_str() == d.rule_id.as_ref()))
        .collect();

    let after_overrides = apply_config_filters(requested, &config);
    let after_suppressions =
        ignore_comments::apply_to_all(after_overrides, &discovered, &clean_files);
    let has_violations = !after_suppressions.is_empty();

    if should_emit_json {
        report_diagnostics_json(&after_suppressions)?;
    } else {
        report_diagnostics(&after_suppressions);
    }
    Ok(has_violations)
}

/// Top-level lint orchestrator. Returns `true` if any violation was reported.
fn lint_project(cli: &Cli) -> Result<bool> {
    let mut timings = Timings::default();
    let t_total = Instant::now();

    let mode = cli.scan_mode();
    let t_discovery = Instant::now();
    let discovered = files::discover(&mode)?;
    timings.discovery = t_discovery.elapsed();

    if discovered.is_empty() {
        if !cli.should_emit_json {
            println!("comply: no files to lint");
        } else {
            println!("[]");
        }
        timings.total = t_total.elapsed();
        if cli.timings {
            print_timings(&timings);
        }
        return Ok(false);
    }

    // Look for `comply.toml` starting from the first discovered file's
    // directory rather than from `cwd`. This makes `comply some/path/x.ts`
    // pick up `some/path/comply.toml` (or a parent's) without requiring
    // the user to `cd` into the project first.
    let t_config = Instant::now();
    let config_anchor = discovered
        .first()
        .and_then(|f| f.path.parent())
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let config = Config::load_from(&config_anchor)?;
    timings.config = t_config.elapsed();

    // --fix runs the upstream fixers (oxlint --fix, cargo clippy
    // --fix) over the discovered files BEFORE the lint pass. After
    // the editors finish, comply re-reads the files via the normal
    // pipeline so the user sees what's left — typically the
    // architectural diagnostics no fixer can address.
    if cli.fix && !cli.comply_only {
        let t_fix = Instant::now();
        let runs = fix::apply_fixes(&discovered, &config)?;
        timings.fix = t_fix.elapsed();
        eprintln!("comply: ran {runs} fixer(s); re-linting");
    }

    let (diagnostics, clean_files) = collect_all_diagnostics(
        &discovered,
        &config,
        &mut timings,
        cli.comply_only,
        cli.type_aware,
        cli.is_partial_scan(),
    )?;

    let t_post = Instant::now();
    let after_overrides = apply_config_filters(diagnostics, &config);
    let mut after_suppressions =
        ignore_comments::apply_to_all(after_overrides, &discovered, &clean_files);
    if cli.diff_only {
        let changed = changed_lines::changed_lines(&mode)?;
        let repo_root = changed_lines::git_repo_root().unwrap_or_default();
        for diag in &mut after_suppressions {
            let normalised = changed_lines::normalise_path(diag.path.as_ref(), &repo_root);
            diag.path = std::sync::Arc::from(normalised);
        }
        changed_lines::retain_in_diff(&mut after_suppressions, &changed);
    }
    timings.post = t_post.elapsed();

    let has_violations = !after_suppressions.is_empty();

    if cli.tui {
        if !std::io::stdout().is_terminal() {
            eprintln!("comply: --tui requires an interactive terminal");
            std::process::exit(2);
        }
        if has_violations {
            let paths: std::collections::HashSet<&std::path::Path> =
                after_suppressions.iter().map(|d| d.path.as_ref()).collect();
            let sources: std::collections::HashMap<std::sync::Arc<std::path::Path>, String> = paths
                .into_iter()
                .map(|p| {
                    let content = std::fs::read_to_string(p).unwrap_or_default();
                    (std::sync::Arc::from(p), content)
                })
                .collect();
            let display_root = match &mode {
                ScanMode::All(path) => {
                    if path.is_file() {
                        path.parent().unwrap_or(path).to_path_buf()
                    } else {
                        path.clone()
                    }
                }
                _ => std::env::current_dir().unwrap_or_default(),
            };
            tui::run(after_suppressions, sources, display_root, config.theme())?;
        } else {
            println!("comply: all clear");
        }
    } else if cli.should_emit_json {
        report_diagnostics_json(&after_suppressions)?;
    } else {
        report_diagnostics(&after_suppressions);
    }
    timings.total = t_total.elapsed();
    if cli.timings {
        print_timings(&timings);
    }
    Ok(has_violations)
}

/// Apply every linter (oxlint + custom rules) and collect diagnostics.
fn collect_all_diagnostics(
    discovered: &[SourceFile],
    config: &Config,
    timings: &mut Timings,
    is_comply_only: bool,
    type_aware: bool,
    is_partial: bool,
) -> Result<(Vec<Diagnostic>, std::collections::HashSet<std::path::PathBuf>)> {
    let by_lang = partition_by_language(discovered);
    let mut diagnostics = Vec::with_capacity(discovered.len());

    // In diff modes the discovered set only contains changed files.
    // Cross-file rules (unused-dependency, dead-export, unused-file) need
    // a complete import index, so walk the full project tree for the
    // ProjectCtx when running a partial scan.
    let full_project_files: Vec<SourceFile>;
    let index_refs: Vec<&SourceFile> = if is_partial {
        let cwd = std::env::current_dir().unwrap_or_default();
        let start = discovered
            .first()
            .map(|f| {
                let p = f.path.parent().unwrap_or(std::path::Path::new("."));
                if p.as_os_str().is_empty() { &cwd } else { p }
            })
            .unwrap_or(&cwd);
        let root = crate::project::walk_up_finding(start, "package.json")
            .or_else(|| crate::project::walk_up_finding(start, ".git"))
            .unwrap_or_else(|| start.to_path_buf());
        let root = if root.as_os_str().is_empty() {
            std::path::PathBuf::from(".")
        } else {
            root
        };
        full_project_files = files::discover(&cli::ScanMode::All(root))?;
        full_project_files.iter().collect()
    } else {
        full_project_files = Vec::new();
        discovered.iter().collect()
    };
    let type_program_ts: Vec<&SourceFile> = if is_partial && type_aware {
        full_project_files
            .iter()
            .filter(|f| f.language.is_typescript_family())
            .collect()
    } else {
        Vec::new()
    };

    let clones_enabled =
        discovered.len() >= 2 && !config.is_rule_globally_disabled(clone_detection::RULE_ID);
    let dup_comments_enabled = discovered.len() >= 2
        && !config.is_rule_globally_disabled(comment_dup_detection::RULE_ID);

    // Clone detection only needs the file list, not the import index, so its
    // `rayon::join` arm runs concurrently with the other arm's full chain:
    // `ProjectCtx::load` (index build) followed by the language engines.
    // Building the index is no longer a serial prelude — it overlaps the
    // clones' tree-sitter tokenize pass, and the engine overlaps the clones'
    // sequential tail as before. The engine arm owns the project and returns
    // it so it escapes for the post-filtering passes below.
    let engine_work = || -> Result<(std::sync::Arc<crate::project::ProjectCtx>, Vec<Diagnostic>)> {
        let project =
            std::sync::Arc::new(crate::project::ProjectCtx::load(&index_refs, config));

        if is_partial {
            let linted: Vec<std::path::PathBuf> = discovered
                .iter()
                .filter_map(|f| std::fs::canonicalize(&f.path).ok())
                .collect();
            project.set_linted_paths(linted);
        }

        let mut diags = Vec::with_capacity(discovered.len());
        if !by_lang.ts.is_empty() {
            diags.extend(lint_typescript(
                &by_lang.ts,
                config,
                &project,
                timings,
                is_comply_only,
                type_aware,
                &type_program_ts,
            )?);
        }
        if !by_lang.rs.is_empty() {
            diags.extend(lint_rust(
                &by_lang.rs,
                config,
                &project,
                timings,
                is_comply_only,
            )?);
        }
        if !by_lang.vue.is_empty() {
            let t = Instant::now();
            let vue_diags = engine::lint_files_with_project(&by_lang.vue, config, &project, None)?;
            timings.engine_vue = t.elapsed();
            diags.extend(vue_diags);
        }
        if !by_lang.json.is_empty() {
            diags.extend(engine::lint_files_with_project(
                &by_lang.json,
                config,
                &project,
                None,
            )?);
        }
        Ok((project, diags))
    };
    let clones_work = || -> (Vec<Diagnostic>, std::time::Duration) {
        if !clones_enabled {
            return (Vec::new(), std::time::Duration::ZERO);
        }
        let t = Instant::now();
        let all_refs: Vec<&SourceFile> = discovered.iter().collect();
        let d = clone_detection::lint_files(&all_refs);
        (d, t.elapsed())
    };
    let dup_comments_work = || -> Vec<Diagnostic> {
        if !dup_comments_enabled {
            return Vec::new();
        }
        let all_refs: Vec<&SourceFile> = discovered.iter().collect();
        comment_dup_detection::lint_files(&all_refs, config)
    };

    let (engine_res, ((clone_diags, clones_elapsed), dup_diags)) =
        rayon::join(engine_work, || rayon::join(clones_work, dup_comments_work));
    let (project, engine_diags) = engine_res?;
    diagnostics.extend(engine_diags);
    diagnostics.extend(clone_diags);
    diagnostics.extend(dup_diags);
    timings.clones = clones_elapsed;

    if project.has_framework("drizzle") {
        diagnostics.retain(|d| {
            !(d.rule_id.as_ref() == "oxc/no-barrel-file"
                && d.path.file_name().is_some_and(|n| n == "schema.ts"))
        });
    }

    let clean_files = project.clean_files_snapshot();
    Ok((diagnostics, clean_files))
}

/// Apply config-driven filters to subprocess diagnostics (oxlint, clippy)
/// where the rule already ran but the user wants it dropped or its
/// severity changed. Tree-sitter rules are filtered upstream in the engine.
///
/// We need this post-filter because oxlint/clippy don't know about
/// per-glob `disable = [...]` overrides — they run their full lint set
/// and we filter the resulting diagnostics by `(rule_id, file_path)`.
fn apply_config_filters(mut diagnostics: Vec<Diagnostic>, config: &Config) -> Vec<Diagnostic> {
    diagnostics.retain(|d| config.is_rule_enabled(d.rule_id.as_ref(), d.path.as_ref()));
    for d in &mut diagnostics {
        if let Some(override_sev) = config.severity_for(d.rule_id.as_ref()) {
            d.severity = override_sev;
        }
    }
    diagnostics
}

/// Lint Rust files via clippy subprocess + custom tree-sitter rules.
/// The two passes are complementary: clippy catches type-aware lints
/// and the standard library footguns; custom rules catch the architecture
/// and naming concerns that clippy doesn't model.
fn lint_rust(
    rs_files: &[&SourceFile],
    config: &Config,
    project: &std::sync::Arc<crate::project::ProjectCtx>,
    timings: &mut Timings,
    is_comply_only: bool,
) -> Result<Vec<Diagnostic>> {
    let clippy_available = !is_comply_only && clippy::is_available();
    let shear_available = !is_comply_only && cargo_shear::is_available();
    let modules_available = !is_comply_only && cargo_modules::is_available();

    if !is_comply_only {
        if !clippy_available {
            eprintln!(
                "comply: cargo clippy not found — skipping clippy-backed rules. \
                 Install with: rustup component add clippy"
            );
        }
        if !shear_available {
            eprintln!(
                "comply: cargo shear not found — skipping unused-dependency rule. \
                 Install with: cargo install cargo-shear"
            );
        }
        if !modules_available {
            eprintln!(
                "comply: cargo modules not found — skipping orphan-module rule. \
                 Install with: cargo install cargo-modules"
            );
        }
    }

    // All four phases below are independent: they read the same input
    // slice but never mutate shared state and each spawns its own
    // subprocess (clippy/shear/modules) or runs a pure in-process AST
    // walk (engine). Running them in parallel is a straightforward win.
    //
    // Caveat: clippy/shear/modules all shell out to `cargo`, which grabs
    // a file-based lock on `target/`. Under contention they partially
    // serialize on that lock — benchmarks still show a meaningful gain
    // because cargo-metadata parsing and I/O overlap between the three.
    //
    // Each closure measures its own wall-clock via `Instant::now()` and
    // returns `(Result<Vec<Diagnostic>>, Duration)` so the main thread
    // can feed phase durations back into `Timings` after the join.
    type PhaseOut = (Result<Vec<Diagnostic>>, Duration);

    let clippy_phase = || -> PhaseOut {
        if !clippy_available {
            return (Ok(Vec::new()), Duration::ZERO);
        }
        let t = Instant::now();
        (clippy::lint_files(rs_files, config), t.elapsed())
    };
    let shear_phase = || -> PhaseOut {
        if !shear_available {
            return (Ok(Vec::new()), Duration::ZERO);
        }
        let t = Instant::now();
        (cargo_shear::lint_files(rs_files), t.elapsed())
    };
    let modules_phase = || -> PhaseOut {
        if !modules_available {
            return (Ok(Vec::new()), Duration::ZERO);
        }
        let t = Instant::now();
        (cargo_modules::lint_files(rs_files), t.elapsed())
    };
    let project2 = std::sync::Arc::clone(project);
    let engine_phase = || -> PhaseOut {
        let t = Instant::now();
        (
            engine::lint_files_with_project(rs_files, config, &project2, None),
            t.elapsed(),
        )
    };

    // rayon::join(|| join(a, b), || join(c, d)) fans out into a
    // balanced binary tree so all four closures execute on rayon's
    // worker pool in parallel when workers are available.
    let ((clippy_out, shear_out), (modules_out, engine_out)) = rayon::join(
        || rayon::join(clippy_phase, shear_phase),
        || rayon::join(modules_phase, engine_phase),
    );

    let mut diagnostics = Vec::new();

    let (clippy_res, clippy_dur) = clippy_out;
    timings.clippy = clippy_dur;
    diagnostics.extend(clippy_res?);

    let (shear_res, shear_dur) = shear_out;
    timings.cargo_shear = shear_dur;
    diagnostics.extend(shear_res?);

    let (modules_res, modules_dur) = modules_out;
    timings.cargo_modules = modules_dur;
    diagnostics.extend(modules_res?);

    let (engine_res, engine_dur) = engine_out;
    timings.engine_rs = engine_dur;
    diagnostics.extend(engine_res?);

    Ok(diagnostics)
}

/// Files grouped by language family for dispatch.
#[derive(Debug)]
struct FilesByLanguage<'a> {
    ts: Vec<&'a SourceFile>,
    rs: Vec<&'a SourceFile>,
    vue: Vec<&'a SourceFile>,
    json: Vec<&'a SourceFile>,
}

fn partition_by_language(discovered: &[SourceFile]) -> FilesByLanguage<'_> {
    FilesByLanguage {
        ts: discovered
            .iter()
            .filter(|f| f.language.is_typescript_family())
            .collect(),
        rs: discovered
            .iter()
            .filter(|f| f.language == Language::Rust)
            .collect(),
        vue: discovered
            .iter()
            .filter(|f| f.language == Language::Vue)
            .collect(),
        json: discovered
            .iter()
            .filter(|f| f.language == Language::Json)
            .collect(),
    }
}

fn lint_typescript(
    ts_files: &[&SourceFile],
    config: &Config,
    project: &std::sync::Arc<crate::project::ProjectCtx>,
    timings: &mut Timings,
    is_comply_only: bool,
    type_aware: bool,
    type_program_ts: &[&SourceFile],
) -> Result<Vec<Diagnostic>> {
    let oxlint_available = !is_comply_only && oxlint::is_available();

    if !is_comply_only && !oxlint_available {
        eprintln!(
            "comply: oxlint not found — skipping oxlint rules. \
             Install with: npm install -g oxlint oxlint-tsgolint"
        );
    }

    type PhaseOut = (Result<Vec<Diagnostic>>, Duration);

    let type_program_opt: Option<&[&SourceFile]> =
        if type_program_ts.is_empty() { None } else { Some(type_program_ts) };

    let project2 = std::sync::Arc::clone(project);
    let oxlint_phase = || -> PhaseOut {
        if !oxlint_available {
            return (Ok(Vec::new()), Duration::ZERO);
        }
        let t = Instant::now();
        (
            oxlint::lint_files(ts_files, config, project, type_aware, type_program_opt),
            t.elapsed(),
        )
    };
    let engine_phase = || -> PhaseOut {
        let t = Instant::now();
        (
            engine::lint_files_with_project(ts_files, config, &project2, None),
            t.elapsed(),
        )
    };

    let (oxlint_out, engine_out) = rayon::join(oxlint_phase, engine_phase);

    let mut diagnostics = Vec::new();

    let (oxlint_res, oxlint_dur) = oxlint_out;
    timings.oxlint = oxlint_dur;
    diagnostics.extend(oxlint_res?);

    let (engine_res, engine_dur) = engine_out;
    timings.engine_ts = engine_dur;
    diagnostics.extend(engine_res?);

    // Type-aware sidecar phase: only when --type-aware is set. Drives a
    // TypeScript checker (typescript-go) through a Node sidecar to run the
    // custom rules that need resolved types. Skipped in --comply-only.
    if type_aware && !is_comply_only {
        let t = Instant::now();
        let res = typeaware::lint_files(ts_files, config);
        timings.type_aware = t.elapsed();
        diagnostics.extend(res?);
    }

    Ok(diagnostics)
}

/// Print diagnostics and a summary line.
///
/// On an interactive terminal we render the miette-powered pretty frames so
/// humans get code context and remediation inline. Piped or redirected
/// stdout (CI, grep, editors parsing output) keeps the ESLint-like form so
/// existing pipelines continue to work untouched.
fn report_diagnostics(diagnostics: &[Diagnostic]) {
    if diagnostics.is_empty() {
        println!("comply: all clear");
        return;
    }
    let formatted = if std::io::stdout().is_terminal() {
        output::render_pretty(diagnostics)
    } else {
        output::format_eslint(diagnostics)
    };
    print!("{formatted}");
    eprintln!(
        "\ncomply: {} violation{} found",
        diagnostics.len(),
        if diagnostics.len() == 1 { "" } else { "s" }
    );
}

/// Print diagnostics as a JSON array — nothing else on stdout so editors
/// and CI tools can pipe the output directly into a parser.
fn report_diagnostics_json(diagnostics: &[Diagnostic]) -> Result<()> {
    let json = output::format_json(diagnostics)?;
    println!("{json}");
    Ok(())
}
