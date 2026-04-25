//! sql-no-union-when-union-all — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_string, RUST_STRING_KINDS};
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
            if !is_sql_string(text) {
                continue;
            }
            if !super::sql_violates_union_all(text) {
                continue;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "Both sides select a primary key — use `UNION ALL` to skip the dedup sort.".into(),
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
    fn flags_union_with_ids() {
        let src = r#"fn f() { let q = "SELECT id, name FROM a UNION SELECT id, name FROM b"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_union_all() {
        let src = r#"fn f() { let q = "SELECT id, name FROM a UNION ALL SELECT id, name FROM b"; }"#;
        assert!(run(src).is_empty());
    }
}
