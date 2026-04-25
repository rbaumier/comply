//! sql-no-float-for-money — TS / JS / TSX backend.

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
            for line in text.lines() {
                if let Some(ft) = super::float_type_for_money_line(line) {
                    diagnostics.push(Diagnostic::at_node(
                        ctx.path,
                        &node,
                        super::META.id,
                        format!(
                            "`{ft}` near a monetary column — use `NUMERIC(precision, scale)` \
                             to avoid floating-point rounding errors."
                        ),
                        Severity::Error,
                    ));
                    // One diagnostic per node is enough.
                    break;
                }
            }
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
    fn flags_float_for_price() {
        let src = r#"const sql = "CREATE TABLE x (price FLOAT NOT NULL)";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_numeric_for_price() {
        let src = r#"const sql = "CREATE TABLE x (price NUMERIC(10, 2) NOT NULL)";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_non_money_float() {
        let src = r#"const sql = "CREATE TABLE x (latitude FLOAT)";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_in_comment() {
        let src = "// price FLOAT NOT NULL\nconst x = 1;";
        assert!(run(src).is_empty());
    }
}
