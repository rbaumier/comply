//! sql-no-truncate-in-app — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::RUST_STRING_KINDS;

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
        if !super::sql_uses_truncate(text) {
            return;
        }
        if !super::looks_like_sql_truncate(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`TRUNCATE` bypasses triggers and audit — use `DELETE FROM` instead.".into(),
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
    fn flags_truncate_table() {
        let src = r#"fn f() { let q = "TRUNCATE TABLE users"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_delete_from() {
        let src = r#"fn f() { let q = "DELETE FROM users"; }"#;
        assert!(run(src).is_empty());
    }
}
