//! rust-unit-error-result backend.
//!
//! Walks every type expression and flags `Result<_, ()>` patterns.
//! We match on the AST, not on text, so it catches the type wherever
//! it appears: function return types, struct fields, type aliases,
//! generic bounds, etc.

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
            // The error type is the second positional argument. If there's
            // only one (e.g. `io::Result<T>`), the error is implicit and we
            // can't reason about it from the AST alone, so we skip.
            let mut cursor = args.walk();
            let positional: Vec<_> = args
                .named_children(&mut cursor)
                .filter(|c| c.kind() != "type_binding")
                .collect();
            if positional.len() < 2 {
                return;
            }
            let err_type = positional[1];
            if err_type.kind() != "unit_type" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-unit-error-result".into(),
                message: "`Result<_, ()>` discards every error detail. \
                          Define a real error type, or return `Option<T>` \
                          if absence is the only failure mode."
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
    fn flags_result_unit_error_in_return() {
        assert_eq!(run_on("fn f() -> Result<i32, ()> { Ok(0) }").len(), 1);
    }

    #[test]
    fn flags_result_unit_error_in_field() {
        assert_eq!(
            run_on("struct S { last: Result<u8, ()> }").len(),
            1
        );
    }

    #[test]
    fn allows_result_with_real_error() {
        assert!(run_on("fn f() -> Result<i32, String> { Ok(0) }").is_empty());
    }

    #[test]
    fn allows_io_result_alias() {
        // `io::Result<T>` only takes one type arg — we can't see the
        // error from the AST so we don't flag it.
        assert!(run_on("fn f() -> io::Result<()> { Ok(()) }").is_empty());
    }
}
