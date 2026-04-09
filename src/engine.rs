//! Rule engine — reads source files and applies all relevant custom rules.
//!
//! How it works:
//! 1. Collect all registered rules from rules::all_rules().
//! 2. For each file, read its contents once.
//! 3. If any applicable rule needs a tree-sitter AST, parse the file with the
//!    appropriate grammar and pass the tree to those rules.
//! 4. Text-only rules go through `check()`, AST rules through `check_tree()`.
//! 5. Collect and return all diagnostics.

use anyhow::{Context, Result};
use std::fs;
use tree_sitter::Parser;

use crate::diagnostic::Diagnostic;
use crate::files::{Language, SourceFile};
use crate::rules;

/// Apply every registered custom rule to the given files.
pub fn lint_files(files: &[&SourceFile]) -> Result<Vec<Diagnostic>> {
    let rules = rules::all_rules();
    let mut diagnostics = Vec::with_capacity(files.len() * 2);
    let mut parser = Parser::new();

    for file in files {
        let file_diags = lint_one_file(file, &rules, &mut parser)?;
        diagnostics.extend(file_diags);
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
    let source_bytes = source.as_bytes();

    // Filter to rules that declare this file's language.
    let applicable: Vec<_> = rules
        .iter()
        .filter(|r| r.languages().contains(&file.language))
        .collect();

    if applicable.is_empty() {
        return Ok(vec![]);
    }

    // Parse tree-sitter AST once if any applicable rule needs it AND we have
    // a grammar for the language. Returns None for languages without a grammar.
    let needs_tree = applicable.iter().any(|r| r.needs_tree());
    let tree = if needs_tree {
        parse_with_grammar(parser, file.language, source_bytes)
    } else {
        None
    };

    let mut diagnostics = Vec::new();
    for rule in &applicable {
        // Text-only rules — always available.
        diagnostics.extend(rule.check(&file.path, &source, file.language));

        // AST rules — only if we successfully parsed a tree.
        if rule.needs_tree()
            && let Some(ref t) = tree
        {
            diagnostics.extend(rule.check_tree(&file.path, source_bytes, t, file.language));
        }
    }

    Ok(diagnostics)
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
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::Rust => {
            // No grammar bundled in v1 — explicit skip prevents the parser
            // from being applied with whatever language was set previously.
            return None;
        }
    };
    parser.set_language(&lang).ok()?;
    parser.parse(source, None)
}
