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

use super::position::all_substitutions_in_identifier_position;

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
                // Skip tagged template literals: `` sql`SELECT … ${x}` `` and
                // similar are parameterised-query APIs — values are bound as
                // positional parameters on the wire, not concatenated into SQL.
                //
                // In tree-sitter-typescript, a tagged template `` tag`…` `` is
                // represented as a `call_expression` whose `arguments` field is
                // the `template_string` directly (no parenthesised argument
                // list). A regular call `f(\`…\`)` puts the template inside an
                // `arguments` node, so its parent is `arguments`, not
                // `call_expression`.
                if let Some(parent) = node.parent() {
                    if parent.kind() == "call_expression"
                        && parent
                            .child_by_field_name("arguments")
                            .is_some_and(|a| a.id() == node.id())
                    {
                        return;
                    }
                }
                let static_text = template_static_text(node, source_bytes);
                if !is_sql_string(&static_text) {
                    return;
                }
                if static_text.contains("$1") || static_text.contains("$2") {
                    return;
                }
                // Every interpolation sits in an identifier position (a relation
                // or column name), which cannot be a bind parameter.
                let fragments = template_fragments(node, source_bytes);
                let fragment_refs: Vec<&str> =
                    fragments.iter().map(String::as_str).collect();
                if all_substitutions_in_identifier_position(&fragment_refs) {
                    return;
                }
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
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
                // When the SQL string is the left operand, the dynamic right
                // operand is appended at its end. If that end is an identifier
                // position (`"... FROM " + table`), the value names a relation
                // and cannot be a bind parameter.
                if left_sql
                    && let Some(prefix) = string_node_text(left, source_bytes)
                    && all_substitutions_in_identifier_position(&[&prefix, ""])
                {
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
                    path: std::sync::Arc::clone(&ctx.path_arc),
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

/// The static text pieces around each interpolation in a `template_string`, in
/// source order: `n + 1` fragments for `n` `template_substitution` children.
/// Each `template_substitution` closes the current fragment and opens the next,
/// so consecutive substitutions yield an empty fragment between them.
fn template_fragments(node: tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut cursor = node.walk();
    let mut fragments = vec![String::new()];
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "string_fragment" => {
                if let Ok(t) = child.utf8_text(source) {
                    fragments
                        .last_mut()
                        .expect("fragments is seeded with one element")
                        .push_str(t);
                }
            }
            "template_substitution" => fragments.push(String::new()),
            _ => {}
        }
    }
    fragments
}

/// The content of a `string` node with its surrounding quote characters
/// stripped, for inspecting what precedes an appended concat operand. Returns
/// `None` for a non-string node or a `template_string` (handled separately).
fn string_node_text(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    if node.kind() != "string" {
        return None;
    }
    let text = node.utf8_text(source).ok()?;
    let trimmed = text
        .strip_prefix(['\'', '"'])
        .and_then(|t| t.strip_suffix(['\'', '"']))
        .unwrap_or(text);
    Some(trimmed.to_string())
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

    // Regression: issue #376 — postgres-js `sql` tagged template literals
    // (`` sql`SELECT … ${value}` ``) are a parameterised-query API.
    // The interpolated value is bound as a positional parameter on the wire,
    // never concatenated into the SQL string.
    #[test]
    fn does_not_flag_postgres_js_sql_tagged_template() {
        let src = r#"
            import { sql } from "postgres";
            const result = await db.execute(
              sql`SELECT * FROM users WHERE id = ${userId}`
            );
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_generic_sql_tagged_template() {
        let src = r#"await db.execute(sql`SELECT * FROM users WHERE id = ${userId}`);"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_untagged_template_literal_with_interpolated_sql() {
        let src = r#"const q = `SELECT * FROM users WHERE id = ${userId}`;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // Issue #3878 — a table name interpolated into an identifier position
    // cannot be a bind parameter, so it is the only possible form.
    #[test]
    fn does_not_flag_table_identifier_in_template_literal() {
        let src = r#"const q = `SELECT * FROM ${tableName}`;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_table_identifier_in_binary_concat() {
        let src = r#"const q = "SELECT * FROM " + tableName;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_dot_qualified_column_in_template_literal() {
        let src = r#"const q = `SELECT a.${col} FROM a`;"#;
        assert!(run_on(src).is_empty());
    }

    // #3878 guard — a value-position interpolation alongside an identifier one
    // is still an injection and must fire.
    #[test]
    fn flags_value_interpolation_even_with_identifier_interpolation_template() {
        let src = r#"const q = `SELECT * FROM ${t} WHERE id = ${userId}`;"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
