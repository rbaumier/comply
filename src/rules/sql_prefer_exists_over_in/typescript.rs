//! sql-prefer-exists-over-in — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;
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
            if !super::contains_in_subquery(text) {
                continue;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "`IN (SELECT ...)` materializes the entire subquery — \
                 use `EXISTS (SELECT 1 ...)` which short-circuits on first match."
                    .into(),
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
    fn flags_in_subquery() {
        let src = r#"const q = "WHERE id IN (SELECT user_id FROM orders)";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_exists() {
        let src = r#"const q = "WHERE EXISTS (SELECT 1 FROM orders WHERE orders.user_id = u.id)";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_in_value_list() {
        let src = r#"const q = "WHERE id IN (1, 2, 3)";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_in_comment() {
        let src = "// IN (SELECT id FROM t)\nconst x = 1;";
        assert!(run(src).is_empty());
    }
}
