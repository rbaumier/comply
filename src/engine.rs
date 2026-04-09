//! Rule engine — reads source files and applies all relevant custom rules.
//!
//! How it works:
//! 1. Collect all registered rules from rules::all_rules().
//! 2. For each file, read its contents once via `lint_one_file`.
//!    Files that aren't valid UTF-8 are skipped with a stderr warning so a
//!    single binary-ish file can't kill the entire scan.
//! 3. If any applicable rule needs a tree-sitter AST, parse with the right
//!    grammar (LANGUAGE_TYPESCRIPT for .ts/.js, LANGUAGE_TSX for .tsx/.jsx).
//! 4. Text-only rules go through `check()`, AST rules through `check_tree()`.
//! 5. Return all collected diagnostics.

use anyhow::{Context, Result};
use std::fs;
use tree_sitter::Parser;

use crate::diagnostic::Diagnostic;
use crate::files::{Language, SourceFile};
use crate::rules;

/// Apply every registered custom rule to the given files.
#[must_use = "diagnostics from custom rules must be reported"]
pub fn lint_files(files: &[&SourceFile]) -> Result<Vec<Diagnostic>> {
    let rules = rules::all_rules();
    let mut diagnostics = Vec::with_capacity(files.len());
    let mut parser = Parser::new();

    for file in files {
        match lint_one_file(file, &rules, &mut parser) {
            Ok(file_diags) => diagnostics.extend(file_diags),
            Err(e) => {
                // Skip-and-warn — one bad file shouldn't kill the whole scan.
                eprintln!("comply: skipping {}: {e:#}", file.path.display());
            }
        }
    }

    Ok(diagnostics)
}

/// Apply every applicable rule to a single file. Parses the AST once if needed.
fn lint_one_file(
    file: &SourceFile,
    rules: &[Box<dyn rules::Rule>],
    parser: &mut Parser,
) -> Result<Vec<Diagnostic>> {
    let source = fs::read_to_string(&file.path)
        .with_context(|| format!("failed to read {}", file.path.display()))?;
    let applicable: Vec<&dyn rules::Rule> = rules
        .iter()
        .filter(|r| r.languages().contains(&file.language))
        .map(AsRef::as_ref)
        .collect();
    if applicable.is_empty() {
        return Ok(vec![]);
    }
    Ok(apply_rules(file, &source, &applicable, parser))
}

/// Apply each rule to the file, parsing the AST once if any rule needs it.
fn apply_rules(
    file: &SourceFile,
    source: &str,
    applicable: &[&dyn rules::Rule],
    parser: &mut Parser,
) -> Vec<Diagnostic> {
    let source_bytes = source.as_bytes();
    let tree = if applicable.iter().any(|r| r.needs_tree()) {
        parse_with_grammar(parser, file.language, source_bytes)
    } else {
        None
    };

    let mut diagnostics = Vec::new();
    for rule in applicable {
        if rule.needs_tree() {
            if let Some(ref t) = tree {
                diagnostics.extend(rule.check_tree(&file.path, source_bytes, t, file.language));
            }
        } else {
            diagnostics.extend(rule.check(&file.path, source, file.language));
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
    let lang = match language {
        // Plain TS/JS — TypeScript grammar handles both (TS is a superset).
        Language::TypeScript | Language::JavaScript => {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        }
        // TSX/JSX needs the JSX-aware grammar — using LANGUAGE_TYPESCRIPT
        // produces ERROR nodes peppered through every JSX expression.
        Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        // No grammar bundled for Rust in v1 — explicit skip prevents the
        // parser from being applied with whatever language was set previously.
        Language::Rust => return None,
    };
    parser.set_language(&lang).ok()?;
    parser.parse(source, None)
}
