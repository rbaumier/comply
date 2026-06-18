//! Rule engine — reads source files and applies every RuleDef backend.
//!
//! How it works:
//! 1. Collect all registered rules from `rules::all_rule_defs()`.
//! 2. For each file, read its contents once via `lint_one_file`. Files that
//!    aren't valid UTF-8 are skipped with a stderr warning so a single
//!    binary-ish file can't kill the entire scan.
//! 3. Pick the backends whose `Language` matches this file.
//! 4. If any TreeSitter backend is applicable, parse with the right grammar
//!    once (LANGUAGE_TYPESCRIPT for .ts/.js, LANGUAGE_TSX for .tsx/.jsx).
//! 5. Dispatch per backend variant: TreeSitter/Text run in-process;
//!    Oxlint/Clippy/Tsc contribute their rule-id to external tools and
//!    their diagnostics are remapped post-hoc.

mod oxc_walk;
mod prefilter;
mod walk;

use oxc_walk::{oxc_pre_enabled, run_oxc_checks};
use prefilter::{PrefilterFinders, build_finders, source_matches_prefilter};
use walk::{run_legacy_checks, run_multiplexed_walk};

use anyhow::{Context, Result};
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tree_sitter::Parser;

use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::files::{Language, SourceFile};
use crate::project::ProjectCtx;
use crate::rules::backend::AstCheck;
use crate::rules::backend::OxcCheck;
use crate::rules::file_ctx::FileCtx;
use crate::rules::{self, RuleDef, backend::Backend, backend::CheckCtx, meta::RuleMeta};

const LARGE_PROJECT_FILE_COUNT: usize = 1_000;
const ENGINE_LARGE_PROJECT_BUDGET: Duration = Duration::from_secs(55);
const ENGINE_MAX_FILE_LINES: usize = 10_000;


/// Pre-computed per-language dispatch table. Built once in `lint_files`,
/// shared read-only across all rayon workers.
///
/// Dispatch keys are tree-sitter `kind_id` values (u16) rather than the
/// node-kind strings — a `kind_id` lookup is one integer hash op and
/// avoids the per-node `node.kind()` string materialization.
/// `interesting` is a Vec<bool> indexed by kind_id, used as a fast
/// pre-filter in the walk to skip the closure entirely for uninteresting
/// nodes (most nodes in any tree).
pub(crate) struct LangDispatch<'a> {
    applicable: Vec<(&'a RuleMeta, &'a Backend)>,
    applicable_prefilters: Vec<Option<PrefilterFinders>>,
    multiplexed: Vec<(&'a RuleMeta, &'a dyn AstCheck)>,
    multiplexed_prefilters: Vec<Option<PrefilterFinders>>,
    legacy: Vec<(&'a RuleMeta, &'a dyn AstCheck)>,
    legacy_prefilters: Vec<Option<PrefilterFinders>>,
    dispatch: Vec<Vec<usize>>,
    interesting: Vec<bool>,
    oxc_rules: Vec<(&'a RuleMeta, &'a dyn OxcCheck)>,
    oxc_prefilters: Vec<Option<PrefilterFinders>>,
    /// Path-independent `config.is_rule_enabled` for each oxc rule, valid only
    /// when `globs_empty`. Lets `run_oxc_checks` skip the per-file config
    /// lookup (one `HashMap` hit per rule × file) on the common case of a
    /// project with no per-glob `[overrides]`.
    oxc_config_enabled: Vec<bool>,
    globs_empty: bool,
    has_ts_rules: bool,
}

impl<'a> LangDispatch<'a> {
    pub(crate) fn build(
        rule_defs: &'a [RuleDef],
        language: Language,
        project: &ProjectCtx,
        config: &Config,
    ) -> Self {
        let mut applicable = collect_applicable(rule_defs, language);
        applicable.retain(|(meta, _)| !should_skip_framework_scoped_rule(meta, project));
        let applicable_prefilters: Vec<Option<PrefilterFinders>> = applicable
            .iter()
            .map(|(_, backend)| match backend {
                Backend::TreeSitter(c) => c.prefilter().map(build_finders),
                Backend::Text(c) => c.prefilter().map(build_finders),
                Backend::Oxc(c) => c.prefilter().map(build_finders),
                _ => None,
            })
            .collect();
        let mut multiplexed: Vec<(&'a RuleMeta, &'a dyn AstCheck)> = Vec::new();
        let mut multiplexed_prefilters: Vec<Option<PrefilterFinders>> = Vec::new();
        let mut legacy: Vec<(&'a RuleMeta, &'a dyn AstCheck)> = Vec::new();
        let mut legacy_prefilters: Vec<Option<PrefilterFinders>> = Vec::new();
        for &(meta, ref backend) in &applicable {
            if let Backend::TreeSitter(check) = backend {
                let check: &dyn AstCheck = &**check;
                let pf = check.prefilter().map(build_finders);
                if check.interested_kinds().is_some() {
                    multiplexed.push((meta, check));
                    multiplexed_prefilters.push(pf);
                } else {
                    legacy.push((meta, check));
                    legacy_prefilters.push(pf);
                }
            }
        }
        let mut oxc_rules: Vec<(&'a RuleMeta, &'a dyn OxcCheck)> = Vec::new();
        let mut oxc_prefilters: Vec<Option<PrefilterFinders>> = Vec::new();
        for &(meta, ref backend) in &applicable {
            if let Backend::Oxc(check) = backend {
                let check: &dyn OxcCheck = &**check;
                let pf = check.prefilter().map(build_finders);
                oxc_rules.push((meta, check));
                oxc_prefilters.push(pf);
            }
        }
        let ts_lang = crate::parsing::ts_language_for(language);
        let mut entries: Vec<(u16, usize)> = Vec::new();
        if let Some(ref tsl) = ts_lang {
            for (i, (_, check)) in multiplexed.iter().enumerate() {
                for kind in check.interested_kinds().unwrap() {
                    let kid = tsl.id_for_node_kind(kind, true);
                    if kid == 0 {
                        continue;
                    }
                    entries.push((kid, i));
                }
            }
        }
        let max_kind_id = entries.iter().map(|(k, _)| *k).max().unwrap_or(0);
        let mut dispatch: Vec<Vec<usize>> = vec![Vec::new(); max_kind_id as usize + 1];
        for (kid, i) in entries {
            dispatch[kid as usize].push(i);
        }
        let interesting: Vec<bool> = dispatch.iter().map(|v| !v.is_empty()).collect();
        let has_ts_rules = !multiplexed.is_empty() || !legacy.is_empty();
        let globs_empty = config.path_overrides_empty();
        // When there are no per-glob overrides, `is_rule_enabled` ignores the
        // path, so its result is the same for every file — compute it once.
        let oxc_config_enabled: Vec<bool> = oxc_rules
            .iter()
            .map(|(meta, _)| config.is_rule_enabled(meta.id, Path::new("")))
            .collect();
        Self {
            applicable,
            applicable_prefilters,
            multiplexed,
            multiplexed_prefilters,
            legacy,
            legacy_prefilters,
            dispatch,
            interesting,
            oxc_rules,
            oxc_prefilters,
            oxc_config_enabled,
            globs_empty,
            has_ts_rules,
        }
    }
}

/// Per-worker reusable scratch buffers. Created once per rayon thread by
/// `map_init` and reused across every file that thread processes, so the
/// hot allocations (parser, source string, per-rule diag vectors) survive
/// between files instead of being thrown away each iteration.
struct WorkerState {
    parser: Parser,
    enabled: Vec<bool>,
    states: Vec<Option<Box<dyn std::any::Any>>>,
    per_rule_diags: Vec<Vec<Diagnostic>>,
    source_buf: String,
    // Scratch buffers for the oxc dispatch path, reused across files.
    oxc_enabled: Vec<bool>,
    oxc_dispatch: Vec<Vec<usize>>,
    oxc_per_rule_diags: Vec<Vec<Diagnostic>>,
}

impl WorkerState {
    fn new() -> Self {
        Self {
            parser: Parser::new(),
            enabled: Vec::new(),
            states: Vec::new(),
            per_rule_diags: Vec::new(),
            source_buf: String::new(),
            oxc_enabled: Vec::new(),
            oxc_dispatch: Vec::new(),
            oxc_per_rule_diags: Vec::new(),
        }
    }
}

/// Apply every registered rule to the given files.
///
/// `config` is the resolved per-project configuration. We use it to:
///   - skip rules that are globally `disabled = true`
///   - skip rules that match a per-glob `[overrides."..."]` block
///   - thread thresholds through to rules via `CheckCtx`
///   - rewrite each diagnostic's severity if the user set one
#[must_use = "diagnostics from custom rules must be reported"]
#[allow(dead_code)] // Tests use simple entry point.
pub fn lint_files(files: &[&SourceFile], config: &Config) -> Result<Vec<Diagnostic>> {
    let project = Arc::new(ProjectCtx::load(files, config));
    lint_files_with_project(files, config, &project, None)
}

/// Same as `lint_files` but with a pre-built `ProjectCtx` so the import
/// index covers all languages, not just the slice being linted.
#[must_use = "diagnostics from custom rules must be reported"]
pub fn lint_files_with_project(
    files: &[&SourceFile],
    config: &Config,
    project: &Arc<ProjectCtx>,
    rule_filter: Option<&[String]>,
) -> Result<Vec<Diagnostic>> {
    let mut rule_defs = rules::all_rule_defs();
    if let Some(filter) = rule_filter {
        rule_defs.retain(|r| filter.iter().any(|id| id == r.meta.id));
    }

    // Pre-compute dispatch tables once per language instead of per-file.
    let languages: Vec<Language> = files
        .iter()
        .map(|f| f.language)
        .collect::<FxHashSet<_>>()
        .into_iter()
        .collect();
    let lang_dispatches: FxHashMap<Language, LangDispatch> = languages
        .into_iter()
        .map(|lang| (lang, LangDispatch::build(&rule_defs, lang, project, config)))
        .collect();

    let deadline = (files.len() > LARGE_PROJECT_FILE_COUNT)
        .then(|| Instant::now() + ENGINE_LARGE_PROJECT_BUDGET);
    let timed_out = AtomicBool::new(false);
    let mut diagnostics: Vec<Diagnostic> = files
        .par_iter()
        .map_init(WorkerState::new, |worker, file| {
            if let Some(deadline) = deadline
                && Instant::now() > deadline
            {
                timed_out.store(true, Ordering::Relaxed);
                return Vec::new();
            }
            let Some(ld) = lang_dispatches.get(&file.language) else {
                return Vec::new();
            };
            match lint_one_file_with_dispatch(file, ld, worker, config, project) {
                Ok(file_diags) => file_diags,
                Err(e) => {
                    eprintln!("comply: skipping {}: {e:#}", file.path.display());
                    Vec::new()
                }
            }
        })
        .flatten()
        .collect();

    if timed_out.load(Ordering::Relaxed) {
        eprintln!(
            "comply: engine budget reached after {}s on {} file(s); continuing with partial results",
            ENGINE_LARGE_PROJECT_BUDGET.as_secs(),
            files.len()
        );
    }

    diagnostics.retain(|d| !is_self_reference(d));
    dedup_mutation_family(&mut diagnostics);

    Ok(diagnostics)
}

/// Lint in-memory text against every registered rule for `language`.
///
/// Used by the LSP server, where the editor sends us the current
/// document text on every keystroke and we don't want to read from
/// disk (the disk version is stale relative to the editor's buffer).
/// Same dispatch logic as `lint_one_file`, minus the disk read.
///
/// Oxlint/Clippy/Tsc backends don't produce diagnostics in-process, so the
/// LSP path inherits "tree-sitter and text rules only" for free, which is
/// exactly what we want for per-keystroke editor feedback.
#[must_use = "diagnostics from in-memory lint must be reported"]
#[cfg_attr(not(test), allow(dead_code))]
pub fn lint_in_memory(
    path: &std::path::Path,
    language: Language,
    source: &str,
    config: &Config,
    project: Option<&ProjectCtx>,
) -> Vec<Diagnostic> {
    // LSP callers that haven't built a ProjectCtx yet get the empty default:
    // `nearest_*` still walks disk, only eager root fields are absent.
    let empty;
    let project = match project {
        Some(p) => p,
        None => {
            empty = ProjectCtx::empty();
            &empty
        }
    };
    let rule_defs = rules::all_rule_defs();
    let ld = LangDispatch::build(&rule_defs, language, project, config);
    if ld.applicable.is_empty() {
        return Vec::new();
    }
    let file = SourceFile {
        path: path.to_path_buf(),
        language,
    };
    let mut worker = WorkerState::new();
    dispatch_with_lang(&file, source, &ld, &mut worker, config, project)
}

/// Lint in-memory text using a pre-built `LangDispatch`.
///
/// Called by the LSP server, which caches the dispatch table across keystrokes
/// so the full `LangDispatch::build` cost is paid at most once per language
/// per session, not on every keystroke.
#[must_use = "diagnostics from in-memory lint must be reported"]
pub(crate) fn lint_in_memory_with_dispatch(
    path: &std::path::Path,
    language: Language,
    source: &str,
    config: &Config,
    dispatch: &LangDispatch<'_>,
    project: Option<&ProjectCtx>,
) -> Vec<Diagnostic> {
    let file = SourceFile {
        path: path.to_path_buf(),
        language,
    };
    let mut worker = WorkerState::new();
    let empty;
    let project = match project {
        Some(p) => p,
        None => {
            empty = ProjectCtx::empty();
            &empty
        }
    };
    dispatch_with_lang(&file, source, dispatch, &mut worker, config, project)
}

/// Build a `LangDispatch<'static>` for the LSP dispatch cache.
///
/// Uses the globally stored rule definitions so the returned dispatch borrows
/// `'static` data and can be held inside an `Arc` across async tasks.
pub(crate) fn build_dispatch_for_lsp(language: Language, config: &Config) -> LangDispatch<'static> {
    LangDispatch::build(rules::all_rule_defs_static(), language, &ProjectCtx::empty(), config)
}

/// Flatten `RuleDef[]` into `(meta, backend)` pairs that apply to `language`.
fn collect_applicable(rule_defs: &[RuleDef], language: Language) -> Vec<(&RuleMeta, &Backend)> {
    rule_defs
        .iter()
        .flat_map(|r| {
            r.backends
                .iter()
                .filter(move |(lang, _)| *lang == language)
                .map(move |(_, backend)| (&r.meta, backend))
        })
        .collect()
}

/// Dispatch using a pre-computed `LangDispatch`. Only per-file work
/// (path-based rule filtering, parsing, state creation) happens here.
/// Reuses `worker.enabled`, `worker.states`, `worker.per_rule_diags`
/// across files so the multiplexed walk doesn't re-allocate them on
/// every file.
fn dispatch_with_lang(
    file: &SourceFile,
    source: &str,
    ld: &LangDispatch,
    worker: &mut WorkerState,
    config: &Config,
    project: &ProjectCtx,
) -> Vec<Diagnostic> {
    let path = &file.path;

    // Test-only seam to exercise the per-file panic isolation in
    // `lint_one_file_with_dispatch` without depending on a specific
    // parser-crashing input (those are oxc-version-specific and brittle).
    // Compiles to nothing outside `cfg(test)`.
    #[cfg(test)]
    if path
        .file_name()
        .is_some_and(|n| n == "comply_panic_probe.ts")
    {
        panic!("injected per-file analysis panic (test seam)");
    }

    let line_count = source.bytes().filter(|&b| b == b'\n').count() + 1;
    if line_count > ENGINE_MAX_FILE_LINES {
        return Vec::new();
    }

    let file_ctx = FileCtx::build(path, source, file.language, project);
    if file_ctx.is_generated
        || file_ctx.is_minified
        || file_ctx.path_segments.is_vendored
        || file_ctx.path_segments.is_linter_spec_fixture
    {
        return Vec::new();
    }

    // Fresh per-file memos (source_contains hits, line-start index) before any
    // backend (text, tree-sitter, or oxc) reads them. Worker source buffers are
    // reused, so two consecutive files can share a `(ptr, len)`; this reset
    // guarantees no stale entry regardless of which backends run.
    crate::oxc_helpers::reset_file_caches();

    let needs_ast = ld.has_ts_rules
        && ld
            .applicable
            .iter()
            .zip(&ld.applicable_prefilters)
            .any(|((meta, b), pf)| match b {
                Backend::TreeSitter(_) => {
                    meta.applies_to(&file_ctx, path, config)
                        && pf
                            .as_ref()
                            .is_none_or(|f| source_matches_prefilter(source, f))
                }
                _ => false,
            });
    let tree = if needs_ast {
        crate::parsing::parse_with_grammar(&mut worker.parser, file.language, source.as_bytes())
    } else {
        None
    };
    let path_arc: std::sync::Arc<std::path::Path> = std::sync::Arc::from(path.as_path());
    let ctx = CheckCtx {
        path,
        path_arc,
        source,
        config,
        project,
        file: &file_ctx,
        lang: file.language,
    };
    let mut diagnostics = Vec::new();

    for (&(meta, ref backend), pf) in ld.applicable.iter().zip(&ld.applicable_prefilters) {
        if !meta.applies_to(&file_ctx, path, config) {
            continue;
        }
        let mut produced = match backend {
            Backend::Text(check) => {
                if let Some(f) = pf
                    && !source_matches_prefilter(source, f)
                {
                    continue;
                }
                check.check(&ctx)
            }
            Backend::Oxlint { .. }
            | Backend::Clippy { .. }
            | Backend::Tsc { .. }
            | Backend::Tsgolint { .. }
            | Backend::TypeAware => Vec::new(),
            Backend::TreeSitter(_) | Backend::Oxc(_) => continue,
        };
        if let Some(sev) = config.severity_for(meta.id) {
            for d in &mut produced {
                d.severity = sev;
            }
        }
        diagnostics.extend(produced);
    }

    // oxc-based rules -- parse once with oxc_parser if any Oxc backend is
    // enabled for this file. The pre-parse enabled flags (config + skips +
    // prefilter) are computed once here and reused inside run_oxc_checks.
    let oxc_pre = if ld.oxc_rules.is_empty() {
        Vec::new()
    } else {
        oxc_pre_enabled(ld, &ctx)
    };
    let needs_oxc = oxc_pre.iter().any(|&e| e);

    if needs_oxc {
        // The oxc parser can panic on pathological input (e.g. oxc_ast 0.127
        // crashes on some payload sources). A linter must never let one file
        // take down the whole run, so isolate the parse + dispatch: on panic we
        // skip this file's oxc rules with a warning and keep its text and
        // tree-sitter diagnostics (already collected above). `diagnostics` is
        // only appended to at the very end of `run_oxc_checks`, so a panic
        // leaves it untouched — `AssertUnwindSafe` is sound here.
        let oxc_ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::oxc_helpers::with_oxc_parse(source, path, |semantic| {
                run_oxc_checks(ld, semantic, &ctx, config, &oxc_pre, worker, &mut diagnostics);
            });
        }));
        if oxc_ran.is_err() {
            eprintln!(
                "comply: skipping oxc rules for {} (parser panicked); other rules still applied",
                path.display()
            );
        }
    }

    if let Some(ref t) = tree {
        if !ld.multiplexed.is_empty() {
            run_multiplexed_walk(ld, t, &ctx, source, path, config, worker, &mut diagnostics);
        }
        run_legacy_checks(ld, t, &ctx, source, path, config, &mut diagnostics);
    }

    diagnostics
}

fn should_skip_framework_scoped_rule(meta: &RuleMeta, project: &ProjectCtx) -> bool {
    meta.categories.iter().any(|cat| match *cat {
        "elysia" => !project.has_framework("elysia"),
        "drizzle" => !project.has_framework("drizzle"),
        "zod" => !project.has_framework("zod"),
        "better-result" => !project.has_framework("better-result"),
        "better-auth" => !project.has_framework("better-auth"),
        "shadcn" => !project.has_framework("shadcn"),
        "hono" => !project.has_framework("hono"),
        "xstate" => !project.has_framework("xstate"),
        "angular" => !project.has_framework("angular"),
        "nextjs" => !project.has_framework("nextjs"),
        "i18n" => !project.has_framework("i18n"),
        "nuxt" => !project.has_framework("nuxt"),
        "nestjs" => !project.has_framework("nestjs"),
        "svelte" => !project.has_framework("svelte"),
        "graphql" => !project.has_framework("graphql"),
        "tanstack" | "tanstack-start" | "tanstack-query" => {
            !project.has_framework("tanstack-query")
                && !project.has_framework("tanstack-router")
        }
        _ => false,
    })
}



fn lint_one_file_with_dispatch(
    file: &SourceFile,
    ld: &LangDispatch,
    worker: &mut WorkerState,
    config: &Config,
    project: &ProjectCtx,
) -> Result<Vec<Diagnostic>> {
    // Read into the worker's reusable String buffer instead of letting
    // `fs::read_to_string` allocate a fresh one per file. `File::read_to_string`
    // appends, so clear() first keeps capacity and just reuses the heap chunk.
    worker.source_buf.clear();
    std::fs::File::open(&file.path)
        .and_then(|mut f| f.read_to_string(&mut worker.source_buf))
        .with_context(|| format!("failed to read {}", file.path.display()))?;
    // The post-filter re-reads every discovered file solely to run this same
    // substring check; record clean files now so it can skip the read for them.
    if !worker.source_buf.contains("comply-ignore") {
        project.note_clean_file(&file.path);
    }
    if ld.applicable.is_empty() {
        return Ok(vec![]);
    }
    let source = std::mem::take(&mut worker.source_buf);
    // Isolate the whole per-file dispatch behind `catch_unwind`: a linter must
    // never let one pathological file take down the entire run. The oxc parser
    // panics on some inputs (an `oxc_span` assertion), and a tree-sitter rule
    // could panic too; without this net the panic unwinds past the rayon worker
    // and the `--json` writer never runs, so every successfully-analyzed file's
    // findings are lost and stdout is empty. On panic we drop this file's
    // diagnostics and keep going. The inner oxc `catch_unwind` stays so an
    // oxc-only panic still preserves the file's tree-sitter diagnostics; this
    // outer net only catches what the inner one cannot.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dispatch_with_lang(file, &source, ld, worker, config, project)
    }));
    worker.source_buf = source;
    match result {
        Ok(diagnostics) => Ok(diagnostics),
        Err(_) => {
            eprintln!(
                "comply: skipping {} (analysis panicked); other files still analyzed",
                file.path.display()
            );
            Ok(Vec::new())
        }
    }
}

/// Mutation-family rules that all flag the same `.sort()` / `.push()` /
/// `.reverse()` site from a different angle. Most-specific first; at a given
/// (path, line, column) only the lowest-index member survives. See #290.
const MUTATION_FAMILY_PRIORITY: &[&str] = &[
    "no-array-sort-mutation",
    "no-mutating-methods",
    "no-mutation",
];

/// Collapse mutation-family duplicates at the same location to the most
/// specific diagnostic. Non-family diagnostics are untouched.
fn dedup_mutation_family(diagnostics: &mut Vec<Diagnostic>) {
    fn priority(rule_id: &str) -> Option<usize> {
        MUTATION_FAMILY_PRIORITY
            .iter()
            .position(|id| *id == rule_id)
    }
    let mut best: FxHashMap<(Arc<Path>, usize, usize), usize> = FxHashMap::default();
    for (idx, d) in diagnostics.iter().enumerate() {
        let Some(prio) = priority(&d.rule_id) else {
            continue;
        };
        let key = (Arc::clone(&d.path), d.line, d.column);
        let cur_prio = best
            .get(&key)
            .map(|&cur| priority(&diagnostics[cur].rule_id).unwrap_or(usize::MAX));
        if cur_prio.is_none_or(|cur| prio < cur) {
            best.insert(key, idx);
        }
    }
    let kept: FxHashSet<usize> = best.values().copied().collect();
    let mut idx = 0usize;
    diagnostics.retain(|d| {
        let in_family = priority(&d.rule_id).is_some();
        let keep = !in_family || kept.contains(&idx);
        idx += 1;
        keep
    });
}

/// True if the diagnostic's rule fires on its OWN source directory,
/// i.e. `rule_id = "banned-comment-words"` firing on a path containing
/// `src/rules/banned_comment_words/`.
fn is_self_reference(d: &Diagnostic) -> bool {
    let dir_fragment = d.rule_id.as_ref().replace('-', "_");
    let needle = format!("src/rules/{dir_fragment}/");
    let alt_needle = format!("src\\rules\\{dir_fragment}\\");
    let path_str = d.path.to_string_lossy();
    path_str.contains(&needle) || path_str.contains(&alt_needle)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use crate::config::default_static_config;
    use crate::diagnostic::{Diagnostic, Severity};
    use crate::engine::{dedup_mutation_family, lint_in_memory};
    use crate::files::Language;
    use crate::project::ProjectCtx;

    fn mk(rule_id: &'static str, line: usize, col: usize) -> Diagnostic {
        Diagnostic {
            path: Arc::from(Path::new("src/foo.ts")),
            line,
            column: col,
            rule_id: rule_id.into(),
            message: rule_id.into(),
            severity: Severity::Warning,
            span: None,
        }
    }

    #[test]
    fn next_no_assign_module_variable_gated_off_outside_nextjs() {
        let meta = &crate::rules::next_no_assign_module_variable::META;
        // Non-Next.js project: `const module = ...` is a legitimate identifier,
        // so the Next.js-only rule must be skipped.
        let non_next = ProjectCtx::empty();
        assert!(
            super::should_skip_framework_scoped_rule(meta, &non_next),
            "rule must be skipped when the project is not Next.js"
        );
        // Next.js project: the rule must still fire.
        let next = ProjectCtx::for_test_with_framework("nextjs");
        assert!(
            !super::should_skip_framework_scoped_rule(meta, &next),
            "rule must still fire on a Next.js project"
        );
    }

    #[test]
    fn next_rules_gated_off_outside_nextjs() {
        // These Next.js-only rules must run only when `nextjs` is detected,
        // via the central framework gate — never on plain Node/Vite/SvelteKit.
        let metas: &[(&str, &crate::rules::meta::RuleMeta)] = &[
            (
                "next-inline-script-id",
                &crate::rules::next_inline_script_id::META,
            ),
            (
                "next-no-duplicate-head",
                &crate::rules::next_no_duplicate_head::META,
            ),
            (
                "next-no-script-component-in-head",
                &crate::rules::next_no_script_component_in_head::META,
            ),
            (
                "next-no-title-in-document-head",
                &crate::rules::next_no_title_in_document_head::META,
            ),
            ("next-no-typos", &crate::rules::next_no_typos::META),
        ];
        let non_next = ProjectCtx::empty();
        let next = ProjectCtx::for_test_with_framework("nextjs");
        for (id, meta) in metas {
            assert!(
                super::should_skip_framework_scoped_rule(meta, &non_next),
                "{id} must be skipped when the project is not Next.js"
            );
            assert!(
                !super::should_skip_framework_scoped_rule(meta, &next),
                "{id} must still fire on a Next.js project"
            );
        }
    }

    #[test]
    fn dedup_mutation_family_keeps_most_specific() {
        let mut diags = vec![
            mk("no-mutation", 30, 18),
            mk("no-mutating-methods", 30, 18),
            mk("no-array-sort-mutation", 30, 18),
        ];
        dedup_mutation_family(&mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-array-sort-mutation");
    }

    #[test]
    fn dedup_mutation_family_collapses_push_pair() {
        // no-array-sort-mutation doesn't fire on .push() — pair of
        // no-mutating-methods + no-mutation collapses to the more specific.
        let mut diags = vec![
            mk("no-mutation", 30, 3),
            mk("no-mutating-methods", 30, 3),
        ];
        dedup_mutation_family(&mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-mutating-methods");
    }

    #[test]
    fn dedup_mutation_family_leaves_unrelated_rules_alone() {
        let mut diags = vec![
            mk("no-mutation", 30, 18),
            mk("no-array-sort-mutation", 30, 18),
            mk("explicit-length-check", 30, 18), // unrelated, same loc
            mk("no-mutation", 31, 18),           // different line — stays
        ];
        dedup_mutation_family(&mut diags);
        let ids: Vec<&str> = diags.iter().map(|d| d.rule_id.as_ref()).collect();
        assert_eq!(
            ids,
            vec!["no-array-sort-mutation", "explicit-length-check", "no-mutation"]
        );
    }

    #[test]
    fn skips_ui_a11y_tailwind_fixture_rules_in_test_files() {
        let source = r#"
export function Fixture() {
  return <button className="z-[9999]" onClick={() => {}}>click</button>;
}
"#;
        let diagnostics = lint_in_memory(
            Path::new("test/use-swr-key.test.tsx"),
            Language::Tsx,
            source,
            default_static_config(),
            None,
        );
        let skipped_rule_ids = [
            "a11y-click-events-have-key-events",
            "html-require-button-type",
            "react-button-has-type",
            "tailwind-require-focus-ring",
            "tailwind-no-arbitrary-z-index",
        ];

        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| !skipped_rule_ids.contains(&diagnostic.rule_id.as_ref())),
            "expected fixture-only rules to stay silent in tests, got: {diagnostics:?}",
        );
    }

    #[test]
    fn skips_all_rules_in_linter_spec_fixtures_issue1438() {
        // Issue #1438: rome-tools/Biome linter spec fixtures (invalid.jsx /
        // valid.jsx under tests/specs/) hold intentional test data — code the
        // linter-under-test should/shouldn't flag — read as text, never imported
        // or bundled. Every rule must skip them; here ts-no-unused-vars and
        // ts-no-empty-function both fire on the same content in a normal file.
        let source = "const unused = 1;\nfunction empty() {}\n";
        let fixture = lint_in_memory(
            Path::new("crates/rome_js_analyze/tests/specs/a11y/noAccessKey/invalid.jsx"),
            Language::JavaScript,
            source,
            default_static_config(),
            None,
        );
        assert!(
            fixture.is_empty(),
            "linter spec fixtures must be exempt from all rules, got: {fixture:?}"
        );

        // Negative space: the SAME content in a normal source file is still
        // flagged — the exemption is scoped to the tests/specs/ fixture path.
        let normal = lint_in_memory(
            Path::new("src/feature.js"),
            Language::JavaScript,
            source,
            default_static_config(),
            None,
        );
        assert!(
            normal
                .iter()
                .any(|d| d.rule_id == "ts-no-unused-vars" || d.rule_id == "ts-no-empty-function"),
            "normal source with the same patterns must still be flagged, got: {normal:?}"
        );

        // Negative space: a `src/valid.ts` (no tests/specs/ ancestor) with an
        // unused var must still be checked — the stem alone must not exempt.
        let plain_valid = lint_in_memory(
            Path::new("src/valid.js"),
            Language::JavaScript,
            "const unused = 1;\n",
            default_static_config(),
            None,
        );
        assert!(
            plain_valid.iter().any(|d| d.rule_id == "ts-no-unused-vars"),
            "src/valid.js (no spec ancestor) must still be checked, got: {plain_valid:?}"
        );
    }

    #[test]
    fn skips_relaxed_directory_rules_in_examples() {
        let source = r#"fn load() -> anyhow::Result<String> {
    let s = std::fs::read_to_string("x")?;
    Ok(s)
}"#;
        let diagnostics = lint_in_memory(
            Path::new("examples/jwt/src/main.rs"),
            Language::Rust,
            source,
            default_static_config(),
            None,
        );

        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.rule_id != "rust-anyhow-context-on-question-mark"),
            "expected relaxed examples to suppress anyhow context lint, got: {diagnostics:?}",
        );
    }

    #[test]
    fn skips_relaxed_directory_api_rules_in_benches() {
        let source = r#"use axum::Router;
fn handler() {
    panic!("bench setup failed");
}"#;
        let diagnostics = lint_in_memory(
            Path::new("benches/benches.rs"),
            Language::Rust,
            source,
            default_static_config(),
            None,
        );

        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.rule_id != "structured-api-error"),
            "expected relaxed benches to suppress API rules, got: {diagnostics:?}",
        );
    }

    /// Every rule must survive multi-byte UTF-8 input without panicking.
    /// Byte-indexed text scanners are easy to write with a raw cursor that
    /// slices the `&str` mid-character (inside a combining mark, an em dash,
    /// an emoji…), which panics with "byte index N is not a char boundary".
    /// This feeds a multi-byte torture corpus through every registered rule,
    /// per language; a panic here names the offending rule's source location.
    ///
    /// The corpus packs the trigger tokens of the big rule families — regex
    /// literals (the largest char-by-char scanner family), JSDoc tags, React
    /// hooks/JSX, and the framework APIs (zod, hono, elysia, drizzle,
    /// tanstack, …) — with multi-byte content placed right next to them, so
    /// the scanners actually run rather than short-circuiting at the prefilter.
    #[test]
    fn no_rule_panics_on_multibyte_source() {
        // Accents (2-byte), em dash (3-byte), CJK (3-byte), emoji (4-byte)
        // and RTL text sit inside strings, comments and regex char classes —
        // any of them lands a byte cursor off a char boundary.
        const TS: &str = r#"
/**
 * Résumé — convertit héÉ 日本語 💡.
 * @param {Stríng} x — la donnée à slugifier.
 * @returns {número} le résultat.
 * @template Té
 * @yields {número}
 * @throws {Erreur} si échec — 💡.
 */
export function* slugí(x: string): string {
  const obj = {
    type: "notFound",
    // problème é — 💡 日本語
    status: 404,
    title: "Pás trouvé 💡",
  };
  const patterns = [
    /[à-ÿ]+/gu,
    /(?<nomé>\d+)—(\d+)/,
    /日本語+/u,
    /💡{2,3}/u,
    /(héllo|wörld)?/i,
    /^—\s*$/m,
    /[^a-z0-9é]+/g,
    new RegExp("héllo (💡)+ 日本語", "gu"),
  ];
  const tpl = `héllo ${x} 日本語 \x41`;
  yield x.normalize("NFD").replaceAll(patterns[0], "");
  return obj.type + tpl + patterns.length;
}

const schéma = z.object({ clé: z.string().min(1), válue: z.number().optional() });
const app = new Hono().get("/résumé", (c) => c.json({ status: 200, data: "é 💡" }));
const elysiaApp = new Elysia().get("/é", () => "héllo 💡 日本語");
const table = pgTable("usérs", { id: serial("id"), nom: text("nóm") });
const q = useQuery({ queryKey: ["é", "💡"], queryFn: async () => fetch("/日本語") });
const auth = betterAuth({ secret: "héllo-💡-日本語" });
const machine = createMachine({ id: "é", initial: "idlé", states: {} });

@Component({ selector: "app-é", template: "<div>💡 日本語</div>" })
class MonComposant {
  constructor() {}
}

async function chargé(): Promise<void> {
  const r = await fetch("/é-💡");
  if (!r.ok) {
    throw new Error("échec — 💡 日本語");
  }
}
"#;
        const TSX: &str = r#"
import { useEffect, useState, useMemo, useCallback, useRef } from "react";

/** Composant résumé — affiche 💡 日本語. @param {Props} props — les props. */
export function Carte({ titré }: { titré: string }): JSX.Element {
  const [état, setÉtat] = useState(0);
  const mémo = useMemo(() => "héllo 💡 " + titré, [titré]);
  const rappel = useCallback(() => setÉtat((n) => n + 1), []);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // effet é — 💡 日本語
    document.title = `Résumé ${état}`;
  }, [état]);
  return (
    <div ref={ref} className="carté-💡 text-sm" onClick={rappel} title="héllo — 日本語">
      <span>{mémo}</span>
      {état > 0 && <p>válue é 💡 {état}</p>}
      <button type="button">Cliqué 日本語</button>
    </div>
  );
}
"#;
        const RUST: &str = r#"
//! Module résumé — 日本語 💡 العربية.
use rustc_hash::FxHashMap;

/// Doc é à ü — convertit héllo.
pub fn slugï(input: &str) -> Result<String, String> {
    // commentaire é 💡 — TODO: rien à faire
    let re = "[à-ÿ]+— 日本語";
    let map: FxHashMap<&str, i32> = FxHashMap::default();
    let s = format!("héllo {} 日本語 {} {}", input, re, map.len());
    let parts: Vec<&str> = s.split('—').collect();
    for (i, part) in parts.iter().enumerate() {
        let _ = (i, part.len());
    }
    if s.is_empty() {
        return Err("échec — 💡".to_string());
    }
    Ok(s)
}

#[derive(Debug, Clone)]
pub struct Donnée {
    pub nom: String,
    pub valeur: i32,
}

pub async fn charger() -> anyhow::Result<()> {
    let _x = std::fs::read_to_string("résumé.txt")?;
    Ok(())
}
"#;
        // Kitchen sink of trigger tokens for the text-only languages, laced
        // with multi-byte content.
        const TEXT: &str = r#"
# résumé — é 💡 日本語
FROM node:20
ENV NAME="héllo 💡"
RUN curl https://x | sh && wget https://y
USER 1000
SELECT * FROM "usérs" WHERE name = 'héllo 💡' LIMIT 10;
CREATE INDEX idx_é ON usérs (nom);
type Query { résumé: String @deprecated } # commentaire é — 日本語
<template><div class="é">{{ válue }} 💡</div></template>
<script setup>const x = "héllo 日本語"</script>
.foo { font-family: "Hélvetica 💡"; color: #fff; }
@media (min-width: 600px) { .bar { content: "é — 💡"; } }
{ "clé": "válue — 💡 日本語", "nömbre": 42 }
name = "héllo — 💡"
[section]
key = "válue é 💡"
"#;

        // A project context that reports every framework as present, so the
        // framework-scoped rules (skipped on an empty project) run too.
        let mut project = ProjectCtx::empty();
        project.detected_frameworks = crate::frameworks::all();

        let cases: &[(Language, &str, &str)] = &[
            (Language::TypeScript, "src/api/torture.ts", TS),
            (Language::JavaScript, "src/api/torture.js", TS),
            (Language::Tsx, "src/components/torture.tsx", TSX),
            (Language::Rust, "src/torture.rs", RUST),
            (Language::Vue, "src/torture.vue", TEXT),
            (Language::Toml, "src/torture.toml", TEXT),
            (Language::Json, "src/torture.json", TEXT),
            (Language::Css, "src/torture.css", TEXT),
            (Language::Yaml, "src/torture.yaml", TEXT),
            (Language::Dockerfile, "Dockerfile", TEXT),
            (Language::Sql, "src/torture.sql", TEXT),
            (Language::GraphQl, "src/torture.graphql", TEXT),
        ];

        for (language, path, source) in cases {
            let _ = lint_in_memory(
                Path::new(path),
                *language,
                source,
                default_static_config(),
                Some(&project),
            );
        }
    }

    #[test]
    fn oversized_file_is_skipped() {
        // A valid TS file of 10 001 lines must be skipped entirely — no diagnostics.
        let mut source = String::from("export const names = [\n");
        for _ in 0..9_999 {
            source.push_str("  \"value\",\n");
        }
        source.push_str("];\n");
        // source now has 10 001 lines (1 header + 9 999 values + 1 closing)

        let project = ProjectCtx::empty();
        let diagnostics = lint_in_memory(
            Path::new("src/generated.ts"),
            Language::TypeScript,
            &source,
            default_static_config(),
            Some(&project),
        );
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for oversized file, got {diagnostics:?}"
        );
    }

    /// Regression for #1440: a panic while analyzing one file (e.g. the oxc
    /// parser tripping an internal assertion) must not take down the whole run.
    /// The other files' diagnostics must still be collected so `--json` can
    /// serialize partial results instead of emitting zero bytes.
    ///
    /// `comply_panic_probe.ts` hits a `cfg(test)` panic seam in
    /// `dispatch_with_lang`; the run must isolate it and still report the throw
    /// in `clean.ts`.
    #[test]
    fn one_file_panic_does_not_lose_other_files() {
        use crate::files::SourceFile;

        let dir = tempfile::TempDir::new().expect("tempdir");
        let panic_path = dir.path().join("comply_panic_probe.ts");
        let normal_path = dir.path().join("clean.ts");
        std::fs::write(&panic_path, "export const x = 1;\n").expect("write probe");
        std::fs::write(
            &normal_path,
            "export function boom() { throw new Error('x'); }\n",
        )
        .expect("write normal");

        let files = vec![
            SourceFile {
                path: panic_path,
                language: Language::TypeScript,
            },
            SourceFile {
                path: normal_path.clone(),
                language: Language::TypeScript,
            },
        ];
        let refs: Vec<&SourceFile> = files.iter().collect();
        let project = Arc::new(ProjectCtx::load(&refs, default_static_config()));

        let diagnostics =
            super::lint_files_with_project(&refs, default_static_config(), &project, None)
                .expect("lint must not error out when one file panics");

        assert!(
            diagnostics.iter().any(|d| d.path.as_ref() == normal_path),
            "the non-panicking file's diagnostics must survive a sibling's panic, got {diagnostics:?}",
        );
    }
}
