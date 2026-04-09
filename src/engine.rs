#![allow(dead_code)] // Called by main orchestrator (task 12).

//! Rule engine — reads source files and runs all applicable custom rules.
//!
//! How it works:
//! 1. Collect all registered rules from rules::all_rules().
//! 2. For each file, read its contents once.
//! 3. If any rule needs a tree-sitter AST, parse the file once with the
//!    appropriate grammar and pass the tree to those rules.
//! 4. Run text-only rules via check(), AST rules via check_tree().
//! 5. Collect and return all diagnostics.

use anyhow::{Context, Result};
use std::fs;
use tree_sitter::Parser;

use crate::diagnostic::Diagnostic;
use crate::files::{Language, SourceFile};
use crate::rules;
#[allow(unused_imports)]
use crate::rules::Rule;

/// Run all applicable custom rules on the given files.
pub fn run_custom_rules(files: &[&SourceFile]) -> Result<Vec<Diagnostic>> {
    let rules = rules::all_rules();
    let mut diagnostics = Vec::new();
    let mut parser = Parser::new();

    for file in files {
        let source = fs::read_to_string(&file.path)
            .with_context(|| format!("failed to read {}", file.path.display()))?;
        let source_bytes = source.as_bytes();

        // Determine which rules apply to this file's language.
        let applicable: Vec<_> = rules
            .iter()
            .filter(|r| r.languages().contains(&file.language))
            .collect();

        if applicable.is_empty() {
            continue;
        }

        // Parse tree-sitter AST once if any applicable rule needs it.
        let needs_tree = applicable.iter().any(|r| r.needs_tree());
        let tree = if needs_tree {
            set_parser_language(&mut parser, file.language)?;
            parser.parse(source_bytes, None)
        } else {
            None
        };

        for rule in &applicable {
            // Text-only rules.
            diagnostics.extend(rule.check(&file.path, &source, file.language));

            // AST rules — only called when the tree was parsed.
            if rule.needs_tree()
                && let Some(ref t) = tree
            {
                diagnostics.extend(rule.check_tree(&file.path, source_bytes, t, file.language));
            }
        }
    }

    Ok(diagnostics)
}

/// Configure the parser for the given language. Called once per language switch.
fn set_parser_language(parser: &mut Parser, language: Language) -> Result<()> {
    let lang = match language {
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::Rust => {
            // Rust tree-sitter grammar not included in v1 — skip.
            return Ok(());
        }
    };
    parser
        .set_language(&lang)
        .context("failed to load tree-sitter grammar")?;
    Ok(())
}
