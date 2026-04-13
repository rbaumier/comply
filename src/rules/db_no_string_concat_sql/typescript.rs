//! db-no-string-concat-sql — TS / JS / TSX backend.
//!
//! Detects `"SELECT ... " + variable` style SQL injection in
//! `binary_expression` nodes. The detection is anchored at the
//! string side(s) of the expression — never at variable names —
//! so identifiers containing SQL-keyword substrings don't false-
//! positive.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::is_sql_string;
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        for node in collect_nodes_of_kinds(tree, &["binary_expression"]) {
            let Some(op) = node.child_by_field_name("operator") else {
                continue;
            };
            if op.utf8_text(source_bytes).unwrap_or("") != "+" {
                continue;
            }
            let Some(left) = node.child_by_field_name("left") else {
                continue;
            };
            let Some(right) = node.child_by_field_name("right") else {
                continue;
            };

            let left_sql = string_node_is_sql(left, source_bytes);
            let right_sql = string_node_is_sql(right, source_bytes);

            // At least one side must be a SQL string AND the other side
            // must be a non-string-literal expression (so this is
            // actual concatenation, not two static strings stuck
            // together).
            let one_side_sql = left_sql || right_sql;
            if !one_side_sql {
                continue;
            }
            let other_side_dynamic = if left_sql {
                !is_string_node(right)
            } else {
                !is_string_node(left)
            };
            if !other_side_dynamic {
                continue;
            }
            // Skip if the SQL string already uses placeholders ($1, $2,
            // $N) — that's a parameterised query and the concatenation
            // is harmless.
            let combined = node.utf8_text(source_bytes).unwrap_or("");
            if combined.contains("$1") || combined.contains("$2") {
                continue;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "db-no-string-concat-sql".into(),
                message: "String concatenation with SQL keywords \
                          — SQL injection risk. Use parameterized queries \
                          (`$1`, `?`) instead."
                    .into(),
                severity: Severity::Error,
            });
        }
        diagnostics
    }
}

fn is_string_node(node: tree_sitter::Node) -> bool {
    matches!(node.kind(), "string" | "template_string")
}

fn string_node_is_sql(node: tree_sitter::Node, source: &[u8]) -> bool {
    if !is_string_node(node) {
        return false;
    }
    let Ok(text) = node.utf8_text(source) else {
        return false;
    };
    is_sql_string(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_concat_with_select() {
        let src = r#"const q = "SELECT * FROM users WHERE id = " + userId;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_parameterised_query() {
        let src = r#"const q = "SELECT * FROM users WHERE id = $1";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_sql_concat() {
        let src = r#"const msg = "hello " + name;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_concat_when_variable_name_contains_keyword_substring() {
        // `userFromDb` contains `from` as a substring but the string side
        // is plain prose — should not flag.
        let src = r#"const msg = "the result was " + userFromDb;"#;
        assert!(run_on(src).is_empty());
    }
}
