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
use std::fs;
use std::sync::Arc;
use tree_sitter::Parser;

use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::files::{Language, SourceFile};
use crate::project::ProjectCtx;
use crate::rules::file_ctx::FileCtx;
use crate::rules::{self, backend::Backend, backend::CheckCtx, meta::RuleMeta, RuleDef};

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

    // Project context loads once at the top of the run. `Arc` so every
    // rayon worker gets a cheap clone of the reference — the inner state
    // (manifest caches, lazy OnceLocks) is shared and synchronised via
    // Mutex / OnceLock internally.
    let project = Arc::new(ProjectCtx::load(files, config));

    // Parallel per-file fan-out. `map_init` gives each rayon worker its
    // own `Parser` — tree_sitter::Parser is !Sync, so we can't share one
    // across threads, but rayon's worker-local init solves this by
    // allocating a Parser per worker thread and reusing it across the
    // files that worker processes. The rule registry (`rule_defs`) and
    // config are read-only and Send+Sync via the trait bounds in
    // `src/rules/backend.rs`.
    //
    // A file that fails to read is logged to stderr and contributes zero
    // diagnostics — same skip-and-warn behavior as the sequential path.
    let mut diagnostics: Vec<Diagnostic> = files
        .par_iter()
        .map_init(Parser::new, |parser, file| {
            match lint_one_file(file, &rule_defs, parser, config, &project) {
                Ok(file_diags) => file_diags,
                Err(e) => {
                    eprintln!("comply: skipping {}: {e:#}", file.path.display());
                    Vec::new()
                }
            }
        })
        .flatten()
        .collect();

    // A rule's own source files (doc comments, tests, and fixture strings)
    // legitimately mention the pattern the rule is designed to flag.
    // Drop any diagnostic where rule R fires on a file under
    // `src/rules/<R>/**` — the path mapping rule-id → directory is
    // deterministic (replace dashes with underscores).
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

/// Dispatch each backend variant to produce diagnostics.
///
/// Per-rule and per-glob filtering happens here, before the rule even
/// runs: a disabled rule's `Check::check` is never called, so the user
/// pays nothing for rules they've turned off. Severity overrides are
/// applied after the diagnostic is produced.
fn dispatch_backends(
    file: &SourceFile,
    source: &str,
    applicable: &[(&RuleMeta, &Backend)],
    parser: &mut Parser,
    config: &Config,
    project: &ProjectCtx,
) -> Vec<Diagnostic> {
    // Cull disabled rules up front so we can decide whether parsing the
    // AST is even worth the cost.
    let active: Vec<&(&RuleMeta, &Backend)> = applicable
        .iter()
        .filter(|(meta, _)| config.is_rule_enabled(meta.id, &file.path))
        .collect();
    if active.is_empty() {
        return Vec::new();
    }

    let needs_ast = active
        .iter()
        .any(|(_, b)| matches!(b, Backend::TreeSitter(_)));
    let tree = if needs_ast {
        crate::parsing::parse_with_grammar(parser, file.language, source.as_bytes())
    } else {
        None
    };

    let file_ctx = FileCtx::build(&file.path, source, file.language, project);
    let ctx = CheckCtx {
        path: &file.path,
        source,
        config,
        project,
        file: &file_ctx,
    };
    let mut diagnostics = Vec::new();
    for (meta, backend) in &active {
        let mut produced = match backend {
            Backend::TreeSitter(check) => {
                if let Some(ref t) = tree {
                    check.check(&ctx, t)
                } else {
                    Vec::new()
                }
            }
            Backend::Text(check) => check.check(&ctx),
            // Oxlint / Clippy / Tsc backends don't produce diagnostics here —
            // they contribute their rule-id to the external tool's config
            // and their diagnostics are remapped in the oxlint/clippy/tsc modules.
            Backend::Oxlint { .. }
            | Backend::Clippy { .. }
            | Backend::Tsc { .. }
            | Backend::Tsgolint { .. } => Vec::new(),
        };
        // Apply severity override if the user set one for this rule.
        if let Some(override_sev) = config.severity_for(meta.id) {
            for d in &mut produced {
                d.severity = override_sev;
            }
        }
        diagnostics.extend(produced);
    }
    diagnostics
}

/// Apply every applicable rule to one file. Parses the AST once if any of
/// the file's applicable backends is a TreeSitter backend.
fn lint_one_file(
    file: &SourceFile,
    rule_defs: &[RuleDef],
    parser: &mut Parser,
    config: &Config,
    project: &ProjectCtx,
) -> Result<Vec<Diagnostic>> {
    let source = fs::read_to_string(&file.path)
        .with_context(|| format!("failed to read {}", file.path.display()))?;

    let applicable = collect_applicable(rule_defs, file.language);
    if applicable.is_empty() {
        return Ok(vec![]);
    }
    Ok(dispatch_backends(file, &source, &applicable, parser, config, project))
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
