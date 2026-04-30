//! sql-no-union-when-union-all — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_string, RUST_STRING_KINDS};

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
        let source_bytes = ctx.source.as_bytes();
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        if !is_sql_string(text) {
            return;
        }
        if !super::sql_violates_union_all(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Both sides select a primary key — use `UNION ALL` to skip the dedup sort.".into(),
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
    fn flags_union_same_table_with_ids() {
        let src = r#"fn f() { let q = "SELECT id, name FROM users UNION SELECT id, name FROM users"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_union_different_tables_with_ids() {
        let src = r#"fn f() { let q = "SELECT id FROM archived_users UNION SELECT id FROM active_users"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_union_all() {
        let src = r#"fn f() { let q = "SELECT id, name FROM a UNION ALL SELECT id, name FROM b"; }"#;
        assert!(run(src).is_empty());
    }
}
