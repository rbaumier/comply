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

use crate::diagnostic::Diagnostic;
use crate::files::{Language, SourceFile};
use crate::rules::{self, backend::Backend, backend::CheckCtx, meta::RuleMeta, RuleDef};

/// Apply every registered rule to the given files.
#[must_use = "diagnostics from custom rules must be reported"]
pub fn lint_files(files: &[&SourceFile]) -> Result<Vec<Diagnostic>> {
    let rule_defs = rules::all_rule_defs();
    let mut diagnostics = Vec::with_capacity(files.len());
    let mut parser = Parser::new();

    for file in files {
        match lint_one_file(file, &rule_defs, &mut parser) {
            Ok(file_diags) => diagnostics.extend(file_diags),
            Err(e) => {
                // Skip-and-warn — one bad file shouldn't kill the whole scan.
                eprintln!("comply: skipping {}: {e:#}", file.path.display());
            }
        }
    }

    Ok(diagnostics)
}

/// Apply every applicable rule to one file. Parses the AST once if any of
/// the file's applicable backends is a TreeSitter backend.
fn lint_one_file(
    file: &SourceFile,
    rule_defs: &[RuleDef],
    parser: &mut Parser,
) -> Result<Vec<Diagnostic>> {
    let source = fs::read_to_string(&file.path)
        .with_context(|| format!("failed to read {}", file.path.display()))?;

    let applicable = collect_applicable(rule_defs, file.language);
    if applicable.is_empty() {
        return Ok(vec![]);
    }
    Ok(dispatch_backends(file, &source, &applicable, parser))
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
fn dispatch_backends(
    file: &SourceFile,
    source: &str,
    applicable: &[(&RuleMeta, &Backend)],
    parser: &mut Parser,
) -> Vec<Diagnostic> {
    let needs_ast = applicable
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
    };
    let mut diagnostics = Vec::new();
    for (_meta, backend) in applicable {
        match backend {
            Backend::TreeSitter(check) => {
                if let Some(ref t) = tree {
                    diagnostics.extend(check.check(&ctx, t));
                }
            }
            Backend::Text(check) => {
                diagnostics.extend(check.check(&ctx));
            }
            // Oxlint / Clippy / Tsc backends don't produce diagnostics here —
            // they contribute their rule-id to the external tool's config
            // and their diagnostics are remapped in the oxlint/clippy/tsc modules.
            Backend::Oxlint { .. } | Backend::Clippy { .. } | Backend::Tsc { .. } => {}
        }
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
        // doesn't cover (boolean-naming, explicit-units, law-of-demeter…).
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
    };
    parser.set_language(&lang).ok()?;
    parser.parse(source, None)
}
