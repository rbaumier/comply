//! sql-no-is-deleted-boolean — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{RUST_STRING_KINDS, is_sql_ddl};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(RUST_STRING_KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Ok(text) = node.utf8_text(ctx.source.as_bytes()) else {
            return;
        };
        if !is_sql_ddl(text) {
            return;
        }
        if !super::sql_uses_is_deleted_boolean(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`is_deleted BOOLEAN` loses the deletion time — use `deleted_at TIMESTAMPTZ NULL` instead.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(src, &Check)
    }

    #[test]
    fn flags_is_deleted_boolean() {
        let src = r#"fn f() { let m = "CREATE TABLE t (is_deleted BOOLEAN NOT NULL)"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_deleted_at_timestamptz() {
        let src = r#"fn f() { let m = "CREATE TABLE t (deleted_at TIMESTAMPTZ NULL)"; }"#;
        assert!(run(src).is_empty());
    }
}
