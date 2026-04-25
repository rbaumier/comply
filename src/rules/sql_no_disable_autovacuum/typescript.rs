//! sql-no-disable-autovacuum — TS / JS / TSX backend.

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
            if !super::sql_disables_autovacuum(text) {
                continue;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "Disabling autovacuum causes bloat and XID wraparound — tune thresholds instead.".into(),
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
    fn flags_disable_autovacuum() {
        let src = r#"const m = "ALTER TABLE t SET (autovacuum_enabled = false)";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_off_variant() {
        let src = r#"const m = "ALTER TABLE t SET (autovacuum_enabled = off)";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_threshold_tuning() {
        let src = r#"const m = "ALTER TABLE t SET (autovacuum_vacuum_scale_factor = 0.01)";"#;
        assert!(run(src).is_empty());
    }
}
