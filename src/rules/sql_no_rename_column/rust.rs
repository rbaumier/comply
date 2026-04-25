//! sql-no-rename-column — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_ddl, RUST_STRING_KINDS};
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        for node in collect_nodes_of_kinds(tree, RUST_STRING_KINDS) {
            let Ok(text) = node.utf8_text(source_bytes) else {
                continue;
            };
            if !is_sql_ddl(text) {
                continue;
            }
            if !super::sql_renames_column(text) {
                continue;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "RENAME COLUMN breaks in-flight queries — use expand-contract (add, dual-write, backfill, drop).".into(),
                Severity::Warning,
            ));
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(src, &Check)
    }

    #[test]
    fn flags_rename_column() {
        let src = r#"fn f() { let m = "ALTER TABLE account RENAME COLUMN email TO email_address;"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_add_column() {
        let src = r#"fn f() { let m = "ALTER TABLE account ADD COLUMN email_address TEXT;"; }"#;
        assert!(run(src).is_empty());
    }
}
