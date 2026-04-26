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

use anyhow::{Context, Result};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use tree_sitter::Parser;

use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::files::{Language, SourceFile};
use crate::project::ProjectCtx;
use crate::rules::backend::AstCheck;
use crate::rules::file_ctx::FileCtx;
use crate::rules::walker::walk_tree;
use crate::rules::{self, backend::Backend, backend::CheckCtx, meta::RuleMeta, RuleDef};

/// Pre-computed per-language dispatch table. Built once in `lint_files`,
/// shared read-only across all rayon workers.
struct LangDispatch<'a> {
    applicable: Vec<(&'a RuleMeta, &'a Backend)>,
    multiplexed: Vec<(&'a RuleMeta, &'a dyn AstCheck)>,
    legacy: Vec<(&'a RuleMeta, &'a dyn AstCheck)>,
    dispatch: HashMap<&'static str, Vec<usize>>,
}

impl<'a> LangDispatch<'a> {
    fn build(rule_defs: &'a [RuleDef], language: Language) -> Self {
        let applicable = collect_applicable(rule_defs, language);
        let mut multiplexed: Vec<(&'a RuleMeta, &'a dyn AstCheck)> = Vec::new();
        let mut legacy: Vec<(&'a RuleMeta, &'a dyn AstCheck)> = Vec::new();
        for &(meta, ref backend) in &applicable {
            if let Backend::TreeSitter(check) = backend {
                let check: &dyn AstCheck = &**check;
                if check.interested_kinds().is_some() {
                    multiplexed.push((meta, check));
                } else {
                    legacy.push((meta, check));
                }
            }
        }
        let mut dispatch: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, (_, check)) in multiplexed.iter().enumerate() {
            for kind in check.interested_kinds().unwrap() {
                dispatch.entry(kind).or_default().push(i);
            }
        }
        Self { applicable, multiplexed, legacy, dispatch }
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
pub fn lint_files(files: &[&SourceFile], config: &Config) -> Result<Vec<Diagnostic>> {
    let rule_defs = rules::all_rule_defs();

    let project = Arc::new(ProjectCtx::load(files, config));

    // Pre-compute dispatch tables once per language instead of per-file.
    let languages: Vec<Language> = files.iter().map(|f| f.language).collect::<std::collections::HashSet<_>>().into_iter().collect();
    let lang_dispatches: HashMap<Language, LangDispatch> = languages
        .into_iter()
        .map(|lang| (lang, LangDispatch::build(&rule_defs, lang)))
        .collect();

    let mut diagnostics: Vec<Diagnostic> = files
        .par_iter()
        .map_init(Parser::new, |parser, file| {
            let Some(ld) = lang_dispatches.get(&file.language) else {
                return Vec::new();
            };
            match lint_one_file_with_dispatch(file, ld, parser, config, &project) {
                Ok(file_diags) => file_diags,
                Err(e) => {
                    eprintln!("comply: skipping {}: {e:#}", file.path.display());
                    Vec::new()
                }
            }
        })
        .flatten()
        .collect();

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
    let mut parser = Parser::new();
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
    dispatch_backends(&file, source, &applicable, &mut parser, config, project)
}

/// Flatten `RuleDef[]` into `(meta, backend)` pairs that apply to `language`.
fn collect_applicable(
    rule_defs: &[RuleDef],
    language: Language,
) -> Vec<(&RuleMeta, &Backend)> {
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
fn dispatch_with_lang(
    file: &SourceFile,
    source: &str,
    ld: &LangDispatch,
    parser: &mut Parser,
    config: &Config,
    project: &ProjectCtx,
) -> Vec<Diagnostic> {
    let path = &file.path;

    let needs_ast = ld.applicable.iter().any(|(meta, b)| {
        matches!(b, Backend::TreeSitter(_)) && config.is_rule_enabled(meta.id, path)
    });
    let tree = if needs_ast {
        crate::parsing::parse_with_grammar(parser, file.language, source.as_bytes())
    } else {
        None
    };

    let file_ctx = FileCtx::build(path, source, file.language, project);
    let ctx = CheckCtx {
        path,
        source,
        config,
        project,
        file: &file_ctx,
    };
    let mut diagnostics = Vec::new();

    for &(meta, ref backend) in &ld.applicable {
        if !config.is_rule_enabled(meta.id, path) {
            continue;
        }
        let mut produced = match backend {
            Backend::Text(check) => check.check(&ctx),
            Backend::Oxlint { .. }
            | Backend::Clippy { .. }
            | Backend::Tsc { .. }
            | Backend::Tsgolint { .. } => Vec::new(),
            Backend::TreeSitter(_) => continue,
        };
        if let Some(sev) = config.severity_for(meta.id) {
            for d in &mut produced {
                d.severity = sev;
            }
        }
        diagnostics.extend(produced);
    }

    if let Some(ref t) = tree {
        // Multiplexed walk — dispatch table is shared, only states are per-file.
        if !ld.multiplexed.is_empty() {
            let enabled: Vec<bool> = ld
                .multiplexed
                .iter()
                .map(|(meta, _)| config.is_rule_enabled(meta.id, path))
                .collect();

            let mut states: Vec<Option<Box<dyn std::any::Any>>> = ld
                .multiplexed
                .iter()
                .enumerate()
                .map(|(i, (_, check))| if enabled[i] { check.create_state() } else { None })
                .collect();
            let mut per_rule_diags: Vec<Vec<Diagnostic>> =
                (0..ld.multiplexed.len()).map(|_| Vec::new()).collect();

            walk_tree(t, |node| {
                if let Some(indices) = ld.dispatch.get(node.kind()) {
                    for &i in indices {
                        if !enabled[i] {
                            continue;
                        }
                        let (_, check) = &ld.multiplexed[i];
                        check.visit_node(
                            node,
                            &ctx,
                            states[i].as_deref_mut(),
                            &mut per_rule_diags[i],
                        );
                    }
                }
            });

            for (i, (meta, check)) in ld.multiplexed.iter().enumerate() {
                if !enabled[i] {
                    continue;
                }
                check.finish(&ctx, states[i].take(), &mut per_rule_diags[i]);
                if let Some(sev) = config.severity_for(meta.id) {
                    for d in &mut per_rule_diags[i] {
                        d.severity = sev;
                    }
                }
                diagnostics.extend(per_rule_diags[i].drain(..));
            }
        }

        for (meta, check) in &ld.legacy {
            if !config.is_rule_enabled(meta.id, path) {
                continue;
            }
            let mut produced = check.check(&ctx, t);
            if let Some(sev) = config.severity_for(meta.id) {
                for d in &mut produced {
                    d.severity = sev;
                }
            }
            diagnostics.extend(produced);
        }
    }

    diagnostics
}

/// Dispatch each backend variant to produce diagnostics.
/// Used by the LSP path (`lint_in_memory`) which doesn't pre-build
/// a `LangDispatch`.
fn dispatch_backends(
    file: &SourceFile,
    source: &str,
    _applicable: &[(&RuleMeta, &Backend)],
    parser: &mut Parser,
    config: &Config,
    project: &ProjectCtx,
) -> Vec<Diagnostic> {
    let rule_defs = rules::all_rule_defs();
    let ld = LangDispatch::build(&rule_defs, file.language);
    dispatch_with_lang(file, source, &ld, parser, config, project)
}

fn lint_one_file_with_dispatch(
    file: &SourceFile,
    ld: &LangDispatch,
    parser: &mut Parser,
    config: &Config,
    project: &ProjectCtx,
) -> Result<Vec<Diagnostic>> {
    let source = fs::read_to_string(&file.path)
        .with_context(|| format!("failed to read {}", file.path.display()))?;
    if ld.applicable.is_empty() {
        return Ok(vec![]);
    }
    Ok(dispatch_with_lang(file, &source, ld, parser, config, project))
}

/// True if the diagnostic's rule fires on its OWN source directory,
/// i.e. `rule_id = "banned-comment-words"` firing on a path containing
/// `src/rules/banned_comment_words/`.
fn is_self_reference(d: &Diagnostic) -> bool {
    let dir_fragment = d.rule_id.replace('-', "_");
    let needle = format!("src/rules/{dir_fragment}/");
    let alt_needle = format!("src\\rules\\{dir_fragment}\\");
    let path_str = d.path.to_string_lossy();
    path_str.contains(&needle) || path_str.contains(&alt_needle)
}
