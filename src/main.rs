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
mod changed_lines;
mod cli;
mod clippy;
mod clone_detection;
mod config;
mod runner_helpers;
mod catalog;
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
mod parsing;
mod oxlint;
mod oxlint_config;
mod project;
mod rules;
mod tui;

use std::io::IsTerminal;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command, ConfigAction};
use config::Config;
use diagnostic::Diagnostic;
use files::{Language, SourceFile};

fn main() -> ExitCode {
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
    if cli.fix {
        let t_fix = Instant::now();
        let runs = fix::apply_fixes(&discovered, &config)?;
        timings.fix = t_fix.elapsed();
        eprintln!("comply: ran {runs} fixer(s); re-linting");
    }

    let diagnostics = collect_all_diagnostics(&discovered, &config, &mut timings)?;

    let t_post = Instant::now();
    let after_overrides = apply_config_filters(diagnostics, &config);
    let mut after_suppressions = ignore_comments::apply_to_all(after_overrides, &discovered);
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
            let paths: std::collections::HashSet<&std::path::Path> = after_suppressions
                .iter()
                .map(|d| d.path.as_ref())
                .collect();
            let sources: std::collections::HashMap<std::sync::Arc<std::path::Path>, String> = paths
                .into_iter()
                .map(|p| {
                    let content = std::fs::read_to_string(p).unwrap_or_default();
                    (std::sync::Arc::from(p), content)
                })
                .collect();
            tui::run(after_suppressions, sources)?;
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
) -> Result<Vec<Diagnostic>> {
    let by_lang = partition_by_language(discovered);
    let mut diagnostics = Vec::with_capacity(discovered.len());

    // Build a single ProjectCtx from ALL files so the ImportIndex covers
    // every language — Vue imports from TS (and vice-versa) are resolved.
    let all_refs: Vec<&SourceFile> = discovered.iter().collect();
    let project = std::sync::Arc::new(crate::project::ProjectCtx::load(&all_refs, config));

    if !by_lang.ts.is_empty() {
        diagnostics.extend(lint_typescript(&by_lang.ts, config, &project, timings)?);
    }
    if !by_lang.rs.is_empty() {
        diagnostics.extend(lint_rust(&by_lang.rs, config, timings)?);
    }
    if !by_lang.vue.is_empty() {
        let t = Instant::now();
        let vue_diags = engine::lint_files_with_project(&by_lang.vue, config, &project)?;
        timings.engine_vue = t.elapsed();
        diagnostics.extend(vue_diags);
    }
    if !by_lang.json.is_empty() {
        diagnostics.extend(engine::lint_files_with_project(&by_lang.json, config, &project)?);
    }

    if discovered.len() >= 2 {
        let t = Instant::now();
        let all_refs: Vec<&SourceFile> = discovered.iter().collect();
        diagnostics.extend(clone_detection::lint_files(&all_refs));
        timings.clones = t.elapsed();
    }

    Ok(diagnostics)
}

/// Apply config-driven filters to subprocess diagnostics (oxlint, clippy)
/// where the rule already ran but the user wants it dropped or its
/// severity changed. Tree-sitter rules are filtered upstream in the engine.
///
/// We need this post-filter because oxlint/clippy don't know about
/// per-glob `disable = [...]` overrides — they run their full lint set
/// and we filter the resulting diagnostics by `(rule_id, file_path)`.
fn apply_config_filters(
    mut diagnostics: Vec<Diagnostic>,
    config: &Config,
) -> Vec<Diagnostic> {
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
    timings: &mut Timings,
) -> Result<Vec<Diagnostic>> {
    // Phase availability is cached in OnceLock inside each module, so
    // these is_available() calls are ~free and safe to call outside the
    // parallel region.
    let clippy_avail = clippy::is_available();
    let shear_avail = cargo_shear::is_available();
    let modules_avail = cargo_modules::is_available();

    if !clippy_avail {
        eprintln!(
            "comply: cargo clippy not found — skipping clippy-backed rules. \
             Install with: rustup component add clippy"
        );
    }
    if !shear_avail {
        eprintln!(
            "comply: cargo shear not found — skipping unused-dependency rule. \
             Install with: cargo install cargo-shear"
        );
    }
    if !modules_avail {
        eprintln!(
            "comply: cargo modules not found — skipping orphan-module rule. \
             Install with: cargo install cargo-modules"
        );
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
        if !clippy_avail {
            return (Ok(Vec::new()), Duration::ZERO);
        }
        let t = Instant::now();
        (clippy::lint_files(rs_files, config), t.elapsed())
    };
    let shear_phase = || -> PhaseOut {
        if !shear_avail {
            return (Ok(Vec::new()), Duration::ZERO);
        }
        let t = Instant::now();
        (cargo_shear::lint_files(rs_files), t.elapsed())
    };
    let modules_phase = || -> PhaseOut {
        if !modules_avail {
            return (Ok(Vec::new()), Duration::ZERO);
        }
        let t = Instant::now();
        (cargo_modules::lint_files(rs_files), t.elapsed())
    };
    let engine_phase = || -> PhaseOut {
        let t = Instant::now();
        (engine::lint_files(rs_files, config), t.elapsed())
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
        ts: discovered.iter().filter(|f| f.language.is_typescript_family()).collect(),
        rs: discovered.iter().filter(|f| f.language == Language::Rust).collect(),
        vue: discovered.iter().filter(|f| f.language == Language::Vue).collect(),
        json: discovered.iter().filter(|f| f.language == Language::Json).collect(),
    }
}

fn lint_typescript(
    ts_files: &[&SourceFile],
    config: &Config,
    project: &std::sync::Arc<crate::project::ProjectCtx>,
    timings: &mut Timings,
) -> Result<Vec<Diagnostic>> {
    let oxlint_avail = oxlint::is_available();

    if !oxlint_avail {
        eprintln!(
            "comply: oxlint not found — skipping oxlint rules. \
             Install with: npm install -g oxlint oxlint-tsgolint"
        );
    }

    type PhaseOut = (Result<Vec<Diagnostic>>, Duration);

    let project2 = std::sync::Arc::clone(project);
    let oxlint_phase = || -> PhaseOut {
        if !oxlint_avail {
            return (Ok(Vec::new()), Duration::ZERO);
        }
        let t = Instant::now();
        (oxlint::lint_files(ts_files, config), t.elapsed())
    };
    let engine_phase = || -> PhaseOut {
        let t = Instant::now();
        (engine::lint_files_with_project(ts_files, config, &project2), t.elapsed())
    };

    let (oxlint_out, engine_out) = rayon::join(oxlint_phase, engine_phase);

    let mut diagnostics = Vec::new();

    let (oxlint_res, oxlint_dur) = oxlint_out;
    timings.oxlint = oxlint_dur;
    diagnostics.extend(oxlint_res?);

    let (engine_res, engine_dur) = engine_out;
    timings.engine_ts = engine_dur;
    diagnostics.extend(engine_res?);

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
