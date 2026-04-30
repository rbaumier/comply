//! sql-no-drop-column-without-expand — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{TS_STRING_KINDS, is_sql_ddl};
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        if super::file_marks_deprecation(ctx.source) {
            return Vec::new();
        }
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        for node in collect_nodes_of_kinds(tree, TS_STRING_KINDS) {
            let Ok(text) = node.utf8_text(source_bytes) else {
                continue;
            };
            if !is_sql_ddl(text) {
                continue;
            }
            if !super::sql_drops_column(text) {
                continue;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "DROP COLUMN without a prior deprecation release breaks running deploys — deprecate first, drop later.".into(),
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
    fn flags_bare_drop_column() {
        let src = r#"const m = "ALTER TABLE account DROP COLUMN legacy_flag;";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_deprecation_marker() {
        let src = "// legacy_flag deprecated in v4.2 — expand-contract complete\nconst m = \"ALTER TABLE account DROP COLUMN legacy_flag;\";";
        assert!(run(src).is_empty());
    }
}
