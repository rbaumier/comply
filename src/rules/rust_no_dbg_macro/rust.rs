//! rust-no-dbg-macro backend.
//!
//! Walks `macro_invocation` nodes and flags any whose macro name is
//! `dbg`. We do NOT exempt tests — even in tests, `dbg!` is debug
//! scaffolding that should be removed once the bug is found.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "macro_invocation" {
                return;
            }
            let Some(macro_node) = node.child_by_field_name("macro") else {
                return;
            };
            let Ok(name) = macro_node.utf8_text(source_bytes) else {
                return;
            };
            if name != "dbg" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-no-dbg-macro".into(),
                message: "`dbg!()` is a debugging aid — remove before \
                          committing. For permanent observability use \
                          `tracing::debug!` with structured fields."
                    .into(),
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
            &CheckCtx::for_test(Path::new("t.rs"), source),
            &tree,
        )
    }

    #[test]
    fn flags_dbg_macro() {
        assert_eq!(run_on("fn f() { dbg!(x); }").len(), 1);
    }

    #[test]
    fn flags_dbg_in_let_binding() {
        assert_eq!(run_on("fn f() { let y = dbg!(compute()); }").len(), 1);
    }

    #[test]
    fn does_not_flag_println() {
        assert!(run_on(r#"fn f() { println!("hi"); }"#).is_empty());
    }
}
