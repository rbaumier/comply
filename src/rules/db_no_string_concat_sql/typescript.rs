//! db-no-string-concat-sql — TS / JS / TSX backend.
//!
//! Detects two forms of dynamic SQL string building:
//!
//! 1. `"SELECT ... " + variable` style concatenation in
//!    `binary_expression` nodes. The detection is anchored at the
//!    string side(s) of the expression — never at variable names —
//!    so identifiers containing SQL-keyword substrings don't false-
//!    positive.
//! 2. `` `SELECT ... ${variable}` `` template literals
//!    (`template_string` nodes) that contain at least one
//!    `template_substitution` child and whose concatenated static
//!    fragments match `is_sql_string`. Template literals without
//!    interpolation are harmless (equivalent to a plain string) and
//!    are not flagged.
//!
//! Both detections skip queries that already use named parameter
//! placeholders (`$1`, `$2`) — those are parameterised and the
//! concatenation / interpolation is harmless.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::is_sql_string;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["template_string", "binary_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        match node.kind() {
            "template_string" => {
                if !template_has_interpolation(node) {
                    return;
                }
                let static_text = template_static_text(node, source_bytes);
                if !is_sql_string(&static_text) {
                    return;
                }
                if static_text.contains("$1") || static_text.contains("$2") {
                    return;
                }
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "db-no-string-concat-sql".into(),
                    message: "Template literal with SQL keywords and \
                              interpolation \u{2014} SQL injection risk. Use \
                              parameterized queries (`$1`, `?`) instead."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            "binary_expression" => {
                let Some(op) = node.child_by_field_name("operator") else {
                    return;
                };
                if op.utf8_text(source_bytes).unwrap_or("") != "+" {
                    return;
                }
                let Some(left) = node.child_by_field_name("left") else {
                    return;
                };
                let Some(right) = node.child_by_field_name("right") else {
                    return;
                };

                let left_sql = string_node_is_sql(left, source_bytes);
                let right_sql = string_node_is_sql(right, source_bytes);

                // At least one side must be a SQL string AND the other side
                // must be a non-string-literal expression (so this is
                // actual concatenation, not two static strings stuck
                // together).
                let one_side_sql = left_sql || right_sql;
                if !one_side_sql {
                    return;
                }
                let other_side_dynamic = if left_sql {
                    !is_string_node(right)
                } else {
                    !is_string_node(left)
                };
                if !other_side_dynamic {
                    return;
                }
                // Skip if the SQL string already uses placeholders ($1, $2,
                // $N) — that's a parameterised query and the concatenation
                // is harmless.
                let combined = node.utf8_text(source_bytes).unwrap_or("");
                if combined.contains("$1") || combined.contains("$2") {
                    return;
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
                    span: None,
                });
            }
            _ => {}
        }
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

/// True if a `template_string` node has at least one `template_substitution`
/// child — i.e. actual interpolation, not just a plain `` `literal` ``.
fn template_has_interpolation(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|c| c.kind() == "template_substitution")
}

/// Concatenate the `string_fragment` children of a `template_string` into
/// a single string, interleaving a space for each `template_substitution`
/// so that `is_sql_string`'s whole-word scan still sees the keywords
/// split around interpolation points.
fn template_static_text(node: tree_sitter::Node, source: &[u8]) -> String {
    let mut cursor = node.walk();
    let mut out = String::new();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "string_fragment" => {
                if let Ok(t) = child.utf8_text(source) {
                    out.push_str(t);
                }
            }
            "template_substitution" => {
                out.push(' ');
            }
            _ => {}
        }
    }
    out
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

    #[test]
    fn flags_template_literal_with_interpolated_select() {
        let src = r#"const q = `SELECT * FROM users WHERE id = ${userId}`;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_template_literal_with_interpolated_update() {
        let src = r#"const q = `UPDATE users SET name = '${name}' WHERE id = 1`;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_flag_plain_template_literal_without_interpolation() {
        // A static template literal is equivalent to a string — not a risk.
        let src = "const q = `SELECT * FROM users`;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_non_sql_template_literal() {
        let src = r#"const greeting = `hello ${name}, welcome`;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_parameterised_template_literal() {
        // `$1` is a parameter placeholder — the interpolated variable is
        // the list of params, not the query shape.
        let src = r#"const q = `SELECT * FROM users WHERE id = $1 ${suffix}`;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_prose_template_literal_with_sql_substring() {
        // `update` appears in the prose but there's no DML/WHERE
        // combination — is_sql_string rejects it.
        let src = r#"const msg = `please update the user record ${userId}`;"#;
        assert!(run_on(src).is_empty());
    }
}
