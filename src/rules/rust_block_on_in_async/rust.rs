//! rust-block-on-in-async backend.
//!
//! Walks `call_expression` nodes whose function path ends in
//! `block_on` and verifies the call is inside an `async fn`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_inside_async_fn;
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "call_expression" {
                return;
            }
            let Some(function) = node.child_by_field_name("function") else {
                return;
            };
            let Ok(text) = function.utf8_text(source_bytes) else {
                return;
            };
            if !text.ends_with("block_on") {
                return;
            }
            if !is_inside_async_fn(node, source_bytes) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-block-on-in-async".into(),
                message: format!(
                    "`{text}(..)` from inside an `async fn` triggers tokio's \
                     `Cannot start a runtime from within a runtime` panic. \
                     Use `.await` on the future instead."
                ),
                severity: Severity::Error,
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
    fn flags_block_on_inside_async() {
        let source = "async fn f() { rt.block_on(other_future()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_futures_block_on_inside_async() {
        let source = "async fn f() { futures::executor::block_on(other()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_block_on_in_main() {
        let source = "fn main() { rt.block_on(server()); }";
        assert!(run_on(source).is_empty());
    }
}
