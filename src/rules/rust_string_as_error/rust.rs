//! rust-string-as-error backend.
//!
//! Walks every `generic_type` and flags `Result<_, String>` patterns.
//! Same approach as `rust-unit-error-result`: AST-only, no scope
//! analysis, so it catches the type wherever it appears (function
//! return types, struct fields, type aliases, etc.).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "generic_type" {
                return;
            }
            let Some(type_node) = node.child_by_field_name("type") else {
                return;
            };
            let Ok(type_text) = type_node.utf8_text(source_bytes) else {
                return;
            };
            if type_text != "Result" && !type_text.ends_with("::Result") {
                return;
            }
            let Some(args) = node.child_by_field_name("type_arguments") else {
                return;
            };
            let mut cursor = args.walk();
            let positional: Vec<_> = args
                .named_children(&mut cursor)
                .filter(|c| c.kind() != "type_binding")
                .collect();
            if positional.len() < 2 {
                return;
            }
            let err_type = positional[1];
            let Ok(err_text) = err_type.utf8_text(source_bytes) else {
                return;
            };
            if err_text.trim() != "String" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-string-as-error".into(),
                message: "`Result<_, String>` is stringly-typed — callers \
                          can't pattern-match failure modes. Define a \
                          proper error enum (use `thiserror::Error`)."
                    .into(),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
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
    fn flags_result_string_error() {
        assert_eq!(run_on("fn f() -> Result<i32, String> { Ok(0) }").len(), 1);
    }

    #[test]
    fn allows_result_with_real_error_type() {
        assert!(run_on("fn f() -> Result<i32, MyError> { Ok(0) }").is_empty());
    }

    #[test]
    fn allows_result_unit_error() {
        // Unit-error is a different rule (`rust-unit-error-result`).
        // This rule only flags String — keep concerns separate.
        assert!(run_on("fn f() -> Result<i32, ()> { Ok(0) }").is_empty());
    }
}
