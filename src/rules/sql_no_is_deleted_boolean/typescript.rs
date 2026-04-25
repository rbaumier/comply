//! sql-no-is-deleted-boolean — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_ddl, TS_STRING_KINDS};
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        for node in collect_nodes_of_kinds(tree, TS_STRING_KINDS) {
            let Ok(text) = node.utf8_text(source_bytes) else {
                continue;
            };
            if !is_sql_ddl(text) {
                continue;
            }
            if !super::sql_uses_is_deleted_boolean(text) {
                continue;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "`is_deleted BOOLEAN` loses the deletion time — use `deleted_at TIMESTAMPTZ NULL` instead.".into(),
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
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_is_deleted_boolean() {
        let src = r#"const m = "CREATE TABLE t (is_deleted BOOLEAN NOT NULL DEFAULT false)";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_deleted_at_timestamptz() {
        let src = r#"const m = "CREATE TABLE t (deleted_at TIMESTAMPTZ NULL)";"#;
        assert!(run(src).is_empty());
    }
}
