//! no-abbreviated-names backend for Rust.
//!
//! Same dictionary as the TypeScript impl, applied to Rust identifiers.
//! Splits snake_case words (Rust convention) and checks each against a
//! banned abbreviation list.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const BANNED_ABBREVIATIONS: &[(&str, &str)] = &[
    ("acct", "account"),
    ("usr", "user"),
    ("btn", "button"),
    ("cfg", "config"),
    ("ctx", "context"),
    ("pwd", "password"),
    ("msg", "message"),
    ("req", "request"),
    ("res", "response"),
    ("auth", "authentication"),
    ("idx", "index"),
    ("cnt", "count"),
    ("tmp", "temporary"),
    ("val", "value"),
    ("ret", "return_value"),
    ("num", "number"),
    ("str", "string"),
    ("obj", "object"),
    ("arr", "array"),
    ("dict", "dictionary"),
    ("db", "database"),
    ("err", "error"),
    ("desc", "description"),
    ("addr", "address"),
];

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "identifier" {
                return;
            }
            // Only flag at declaration sites.
            let Some(parent) = node.parent() else {
                return;
            };
            if !matches!(
                parent.kind(),
                "let_declaration" | "parameter" | "function_item" | "const_item" | "static_item"
            ) {
                return;
            }
            let Ok(name) = node.utf8_text(source_bytes) else {
                return;
            };
            let Some((abbr, full)) = matches_banned(name) else {
                return;
            };
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-abbreviated-names".into(),
                message: format!(
                    "Identifier '{name}' contains abbreviation '{abbr}' — \
                     use the full word '{full}'. Editors auto-complete; \
                     readers don't."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

fn matches_banned(name: &str) -> Option<(&'static str, &'static str)> {
    for word in name.split('_') {
        let lower = word.to_ascii_lowercase();
        if let Some(&pair) = BANNED_ABBREVIATIONS.iter().find(|(abbr, _)| lower == *abbr) {
            return Some(pair);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx {
                path: Path::new("t.rs"),
                source,
            },
            &tree,
        )
    }

    #[test]
    fn flags_snake_case_abbreviation() {
        let diags = run_on("fn f() { let user_acct = 1; }");
        assert!(diags.iter().any(|d| d.message.contains("acct")));
    }

    #[test]
    fn flags_bare_abbreviation() {
        let diags = run_on("fn f() { let ctx = 1; }");
        assert!(diags.iter().any(|d| d.message.contains("ctx")));
    }

    #[test]
    fn allows_full_words() {
        assert!(run_on("fn f() { let user_account = 1; }").is_empty());
        assert!(run_on("fn f() { let request_context = 1; }").is_empty());
    }
}
