//! sql-no-disable-autovacuum — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_ddl, RUST_STRING_KINDS};

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
        if !is_sql_ddl(text) {
            return;
        }
        if !super::sql_disables_autovacuum(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Disabling autovacuum causes bloat and XID wraparound — tune thresholds instead.".into(),
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
    fn flags_disable_autovacuum() {
        let src = r#"fn f() { let m = "ALTER TABLE t SET (autovacuum_enabled = false)"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_threshold_tuning() {
        let src = r#"fn f() { let m = "ALTER TABLE t SET (autovacuum_vacuum_scale_factor = 0.01)"; }"#;
        assert!(run(src).is_empty());
    }
}
