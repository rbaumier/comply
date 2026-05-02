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

use oxc_walk::run_oxc_checks;
use prefilter::{PrefilterFinders, build_finders, source_matches_prefilter};
use walk::{run_legacy_checks, run_multiplexed_walk};

use anyhow::{Context, Result};
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::io::Read;
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

/// Pre-computed per-language dispatch table. Built once in `lint_files`,
/// shared read-only across all rayon workers.
///
/// Dispatch keys are tree-sitter `kind_id` values (u16) rather than the
/// node-kind strings — a `kind_id` lookup is one integer hash op and
/// avoids the per-node `node.kind()` string materialization.
/// `interesting` is a Vec<bool> indexed by kind_id, used as a fast
/// pre-filter in the walk to skip the closure entirely for uninteresting
/// nodes (most nodes in any tree).
struct LangDispatch<'a> {
    applicable: Vec<(&'a RuleMeta, &'a Backend)>,
    applicable_prefilters: Vec<Option<PrefilterFinders>>,
    multiplexed: Vec<(&'a RuleMeta, &'a dyn AstCheck)>,
    multiplexed_prefilters: Vec<Option<PrefilterFinders>>,
    legacy: Vec<(&'a RuleMeta, &'a dyn AstCheck)>,
    legacy_prefilters: Vec<Option<PrefilterFinders>>,
    dispatch: FxHashMap<u16, Vec<usize>>,
    interesting: Vec<bool>,
    oxc_rules: Vec<(&'a RuleMeta, &'a dyn OxcCheck)>,
    oxc_prefilters: Vec<Option<PrefilterFinders>>,
    has_ts_rules: bool,
}

impl<'a> LangDispatch<'a> {
    fn build(rule_defs: &'a [RuleDef], language: Language, project: &ProjectCtx) -> Self {
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
        let mut dispatch: FxHashMap<u16, Vec<usize>> = FxHashMap::default();
        let mut max_kind_id: u16 = 0;
        if let Some(ref tsl) = ts_lang {
            for (i, (_, check)) in multiplexed.iter().enumerate() {
                for kind in check.interested_kinds().unwrap() {
                    let kid = tsl.id_for_node_kind(kind, true);
                    // id_for_node_kind returns 0 for unknown kinds (= the ERROR
                    // kind sentinel). Skip those — they'd cause every error
                    // node to dispatch into rules that didn't ask for it.
                    if kid == 0 {
                        continue;
                    }
                    if kid > max_kind_id {
                        max_kind_id = kid;
                    }
                    dispatch.entry(kid).or_default().push(i);
                }
            }
        }
        let mut interesting = vec![false; max_kind_id as usize + 1];
        for &kid in dispatch.keys() {
            interesting[kid as usize] = true;
        }
        let has_ts_rules = !multiplexed.is_empty() || !legacy.is_empty();
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
}

impl WorkerState {
    fn new() -> Self {
        Self {
            parser: Parser::new(),
            enabled: Vec::new(),
            states: Vec::new(),
            per_rule_diags: Vec::new(),
            source_buf: String::new(),
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
        .map(|lang| (lang, LangDispatch::build(&rule_defs, lang, &project)))
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
    Ok(diagnostics)
}

/// Lint in-memory text against every registered rule for `language`.
///
/// Used by the LSP server, where the editor sends us the current
/// document text on every keystroke and we don't want to read from
/// disk (the disk version is stale relative to the editor's buffer).
/// Same dispatch logic as `lint_one_file`, minus the disk read.
///
/// `dispatch_backends` already skips Oxlint/Clippy/Tsc — those backends
/// don't produce diagnostics in-process — so the LSP path inherits
/// "tree-sitter and text rules only" for free, which is exactly what
/// we want for per-keystroke editor feedback.
#[must_use = "diagnostics from in-memory lint must be reported"]
pub fn lint_in_memory(
    path: &std::path::Path,
    language: Language,
    source: &str,
    config: &Config,
    project: Option<&ProjectCtx>,
) -> Vec<Diagnostic> {
    let rule_defs = rules::all_rule_defs();
    let applicable = collect_applicable(&rule_defs, language);
    if applicable.is_empty() {
        return Vec::new();
    }
    let file = SourceFile {
        path: path.to_path_buf(),
        language,
    };
    let mut worker = WorkerState::new();
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
    dispatch_backends(&file, source, &applicable, &mut worker, config, project)
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

    let file_ctx = FileCtx::build(path, source, file.language, project);
    if file_ctx.is_generated || file_ctx.is_minified || file_ctx.path_segments.is_vendored {
        return Vec::new();
    }

    let needs_ast = ld.has_ts_rules
        && ld
            .applicable
            .iter()
            .zip(&ld.applicable_prefilters)
            .any(|((meta, b), pf)| match b {
                Backend::TreeSitter(_) => {
                    config.is_rule_enabled(meta.id, path)
                        && !should_skip_test_fixture_rule(meta, &file_ctx)
                        && !should_skip_relaxed_directory_rule(meta, path)
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
        if !config.is_rule_enabled(meta.id, path) {
            continue;
        }
        if should_skip_test_fixture_rule(meta, &file_ctx) {
            continue;
        }
        if should_skip_relaxed_directory_rule(meta, path) {
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
            | Backend::Tsgolint { .. } => Vec::new(),
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
    // enabled for this file.
    let needs_oxc = !ld.oxc_rules.is_empty()
        && ld
            .oxc_rules
            .iter()
            .zip(&ld.oxc_prefilters)
            .any(|((meta, _), pf)| {
                config.is_rule_enabled(meta.id, path)
                    && !should_skip_test_fixture_rule(meta, &file_ctx)
                    && !should_skip_relaxed_directory_rule(meta, path)
                    && pf
                        .as_ref()
                        .is_none_or(|f| source_matches_prefilter(source, f))
            });

    if needs_oxc {
        crate::oxc_helpers::with_oxc_parse(source, path, |semantic| {
            run_oxc_checks(ld, semantic, &ctx, source, path, config, &mut diagnostics);
        });
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
        "tanstack" | "tanstack-start" | "tanstack-query" => {
            !project.has_framework("tanstack-query")
                && !project.has_framework("tanstack-router")
        }
        _ => false,
    })
}

pub(super) fn should_skip_test_fixture_rule(meta: &RuleMeta, file: &FileCtx) -> bool {
    if !file.path_segments.in_test_dir {
        return false;
    }

    meta.categories
        .iter()
        .any(|category| matches!(*category, "a11y" | "accessibility" | "tailwind" | "ui" | "html"))
        || matches!(meta.id, "react-button-has-type")
}

pub(super) fn should_skip_relaxed_directory_rule(meta: &RuleMeta, path: &std::path::Path) -> bool {
    if !is_relaxed_directory(path) {
        return false;
    }

    meta.categories
        .iter()
        .any(|category| matches!(*category, "api" | "rust" | "security"))
        || matches!(
            meta.id,
            "rust-anyhow-context-on-question-mark" | "rust-serde-deny-unknown-fields"
        )
}

fn is_relaxed_directory(path: &std::path::Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    normalized.starts_with("examples/")
        || normalized.starts_with("benches/")
        || normalized.starts_with("fixtures/")
        || normalized.contains("/examples/")
        || normalized.contains("/benches/")
        || normalized.contains("/fixtures/")
}

/// Dispatch each backend variant to produce diagnostics.
/// Used by the LSP path (`lint_in_memory`) which doesn't pre-build
/// a `LangDispatch`.
fn dispatch_backends(
    file: &SourceFile,
    source: &str,
    _applicable: &[(&RuleMeta, &Backend)],
    worker: &mut WorkerState,
    config: &Config,
    project: &ProjectCtx,
) -> Vec<Diagnostic> {
    let rule_defs = rules::all_rule_defs();
    let ld = LangDispatch::build(&rule_defs, file.language, project);
    dispatch_with_lang(file, source, &ld, worker, config, project)
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
    if ld.applicable.is_empty() {
        return Ok(vec![]);
    }
    // Take the buffer out so we can hand a &str to dispatch_with_lang while
    // still passing &mut worker. Put it back when done so the next file
    // reuses the allocation.
    let source = std::mem::take(&mut worker.source_buf);
    let diagnostics = dispatch_with_lang(file, &source, ld, worker, config, project);
    worker.source_buf = source;
    Ok(diagnostics)
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

    use crate::config::default_static_config;
    use crate::engine::lint_in_memory;
    use crate::files::Language;

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
}
