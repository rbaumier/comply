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
use std::fs;
use tree_sitter::Parser;

use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::files::{Language, SourceFile};
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
    let mut diagnostics = Vec::with_capacity(files.len());
    let mut parser = Parser::new();

    for file in files {
        match lint_one_file(file, &rule_defs, &mut parser, config) {
            Ok(file_diags) => diagnostics.extend(file_diags),
            Err(e) => {
                // Skip-and-warn — one bad file shouldn't kill the whole scan.
                eprintln!("comply: skipping {}: {e:#}", file.path.display());
            }
        }
    }

    // A rule's own source files (doc comments, tests, and fixture strings)
    // legitimately mention the pattern the rule is designed to flag.
    // Drop any diagnostic where rule R fires on a file under
    // `src/rules/<R>/**` — the path mapping rule-id → directory is
    // deterministic (replace dashes with underscores).
    diagnostics.retain(|d| !is_self_reference(d));
    Ok(diagnostics)
}

/// True if the diagnostic's rule fires on its OWN source directory,
/// i.e. `rule_id = "banned-comment-words"` firing on a path containing
/// `src/rules/banned_comment_words/`. Returns `false` for:
///
/// - Diagnostics from rules whose directory name can't be derived from
///   the rule id (there are none today, but keep the mapping strict).
/// - Paths that don't live under `src/rules/*/`.
fn is_self_reference(d: &Diagnostic) -> bool {
    let dir_fragment = d.rule_id.replace('-', "_");
    // We match `src/rules/<dir>/` without a leading slash so BOTH
    // absolute paths (`/Users/.../src/rules/todo_needs_issue_link/text.rs`)
    // and relative paths (`src/rules/todo_needs_issue_link/text.rs`)
    // get caught. Windows path separators are handled by the second
    // needle.
    let needle = format!("src/rules/{dir_fragment}/");
    let alt_needle = format!("src\\rules\\{dir_fragment}\\");
    let path_str = d.path.to_string_lossy();
    path_str.contains(&needle) || path_str.contains(&alt_needle)
}

/// Apply every applicable rule to one file. Parses the AST once if any of
/// the file's applicable backends is a TreeSitter backend.
fn lint_one_file(
    file: &SourceFile,
    rule_defs: &[RuleDef],
    parser: &mut Parser,
    config: &Config,
) -> Result<Vec<Diagnostic>> {
    let source = fs::read_to_string(&file.path)
        .with_context(|| format!("failed to read {}", file.path.display()))?;

    let applicable = collect_applicable(rule_defs, file.language);
    if applicable.is_empty() {
        return Ok(vec![]);
    }
    Ok(dispatch_backends(file, &source, &applicable, parser, config))
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
    dispatch_backends(&file, source, &applicable, &mut parser, config)
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
        parse_with_grammar(parser, file.language, source.as_bytes())
    } else {
        None
    };

    let ctx = CheckCtx {
        path: &file.path,
        source,
        config,
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
            Backend::Oxlint { .. } | Backend::Clippy { .. } | Backend::Tsc { .. } => {
                Vec::new()
            }
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

/// Configure the parser for the language and parse the source.
///
/// Returns None when no tree-sitter grammar is bundled for the language —
/// the caller skips check_tree for those files. Without this explicit None,
/// reusing a parser left in a previous language's state would produce
/// garbage diagnostics from the wrong grammar.
fn parse_with_grammar(
    parser: &mut Parser,
    language: Language,
    source: &[u8],
) -> Option<tree_sitter::Tree> {
    let lang: tree_sitter::Language = match language {
        // Plain TS/JS — TypeScript grammar handles both (TS is a superset).
        Language::TypeScript | Language::JavaScript => {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        }
        // TSX/JSX needs the JSX-aware grammar — using LANGUAGE_TYPESCRIPT
        // produces ERROR nodes peppered through every JSX expression.
        Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        // Rust grammar — enables in-process Rust rules for checks clippy
        // doesn't cover (boolean-naming, explicit-units, …).
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        // Vue SFCs: no bundled grammar. Text-based rules only —
        // returning None skips all TreeSitter backends.
        Language::Vue => return None,
    };
    parser.set_language(&lang).ok()?;
    parser.parse(source, None)
}
