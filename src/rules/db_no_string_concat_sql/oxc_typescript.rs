//! db-no-string-concat-sql oxc backend for TypeScript / JavaScript / TSX.
//!
//! Detects two forms of dynamic SQL string building:
//! 1. `"SELECT ... " + variable` binary concatenation.
//! 2. `` `SELECT ... ${variable}` `` template literals with interpolation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::sql_helpers::is_sql_string;
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TemplateLiteral, AstType::BinaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::TemplateLiteral(tpl) => {
                // Only flag template literals with interpolation.
                if tpl.expressions.is_empty() {
                    return;
                }
                // Skip tagged template literals: `pg`SELECT … ${x}`` and
                // `sql`SELECT … ${x}`` are parameterised-query APIs
                // (postgres-js, Drizzle, Slonik, etc.) — interpolated
                // values are bound as `$1`/`$2` on the wire, not
                // concatenated into the SQL string.
                let parent = semantic.nodes().parent_node(node.id());
                if matches!(parent.kind(), AstKind::TaggedTemplateExpression(_)) {
                    return;
                }
                let static_text: String = tpl
                    .quasis
                    .iter()
                    .map(|q| q.value.raw.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                if !is_sql_string(&static_text) {
                    return;
                }
                if static_text.contains("$1") || static_text.contains("$2") {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, tpl.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "db-no-string-concat-sql".into(),
                    message: "Template literal with SQL keywords and \
                              interpolation \u{2014} SQL injection risk. Use \
                              parameterized queries (`$1`, `?`) instead."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            AstKind::BinaryExpression(bin) => {
                if bin.operator != oxc_ast::ast::BinaryOperator::Addition {
                    return;
                }
                let left_sql = expr_is_sql_string(&bin.left);
                let right_sql = expr_is_sql_string(&bin.right);
                if !left_sql && !right_sql {
                    return;
                }
                // One side must be dynamic (not a string literal).
                let other_side_dynamic = if left_sql {
                    !is_string_expr(&bin.right)
                } else {
                    !is_string_expr(&bin.left)
                };
                if !other_side_dynamic {
                    return;
                }
                // Skip parameterised queries.
                let start = bin.span.start as usize;
                let end = bin.span.end as usize;
                if let Some(combined) = ctx.source.get(start..end)
                    && (combined.contains("$1") || combined.contains("$2")) {
                        return;
                    }
                let (line, column) = byte_offset_to_line_col(ctx.source, start);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "db-no-string-concat-sql".into(),
                    message: "String concatenation with SQL keywords \
                              \u{2014} SQL injection risk. Use parameterized queries \
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

fn is_string_expr(expr: &Expression) -> bool {
    matches!(
        expr.without_parentheses(),
        Expression::StringLiteral(_) | Expression::TemplateLiteral(_)
    )
}

fn expr_is_sql_string(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::StringLiteral(lit) => is_sql_string(lit.value.as_str()),
        Expression::TemplateLiteral(tpl) => {
            let text: String = tpl
                .quasis
                .iter()
                .map(|q| q.value.raw.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            is_sql_string(&text)
        }
        _ => false,
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
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
        let src = r#"const q = `SELECT * FROM users WHERE id = $1 ${suffix}`;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_prose_template_literal_with_sql_substring() {
        let src = r#"const msg = `please update the user record ${userId}`;"#;
        assert!(run_on(src).is_empty());
    }

    // Regression: issue #186 — postgres-js tagged template literals
    // (`` pg`SELECT … ${value}` ``) are a parameterised-query API,
    // structurally identical to Drizzle's `sql` tag. The interpolated
    // value is bound as `$1` on the wire, never concatenated into the
    // SQL string.
    #[test]
    fn does_not_flag_postgres_js_tagged_template() {
        let src = r#"
            import type { Sql } from "postgres";
            async function lockTeamRow(pg: Sql, teamId: string) {
              await pg`SELECT id FROM team WHERE id = ${teamId} FOR UPDATE`;
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_drizzle_sql_tagged_template() {
        let src = r#"await db.execute(sql`SELECT * FROM users WHERE id = ${userId}`);"#;
        assert!(run_on(src).is_empty());
    }

    // Targeted-fix guard: plain template-literal SQL concat (no tag)
    // must still be flagged. Proves the tagged-template skip didn't
    // turn the rule off wholesale.
    #[test]
    fn still_flags_untagged_template_literal_with_interpolated_sql() {
        let src = r#"const q = `SELECT * FROM users WHERE id = ${userId}`;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_binary_concat_sql() {
        let src = r#"const q = "SELECT * FROM users WHERE id = " + userId;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    // Regression for rbaumier/comply#2321 — Sequelize's query-generator
    // tests assert dialect→SQL snapshot objects via an `expectsql()` helper.
    // Some expected strings interpolate dialect-specific schema names into
    // the *expected output* (e.g. `${dialect.getDefaultSchema()}`). These
    // are fixtures compared against, never executed against a database, so
    // there is no injection risk. The engine's `skip_in_test_dir` gate
    // suppresses the rule for any file in a test directory.
    #[test]
    fn gated_no_fp_on_test_snapshot_object() {
        let src = r#"
            it('produces a show constraints query', () => {
              expectsql(() => queryGenerator.showConstraintsQuery('myTable'), {
                postgres: `SELECT c.constraint_name FROM information_schema WHERE c.table_name = 'myTable'`,
                oracle: `SELECT C.CONSTRAINT_NAME FROM ALL_CONSTRAINTS WHERE C.OWNER = '${dialect.getDefaultSchema()}'`,
              });
            });
        "#;
        assert!(
            crate::rules::test_helpers::run_rule_gated(
                &Check,
                src,
                "packages/core/test/unit/query-generator/show-constraints-query.test.ts",
            )
            .is_empty(),
            "skip_in_test_dir must suppress the rule for test-directory files"
        );
    }

    // The same interpolated SQL string at a production path that reaches a
    // query-execution sink is a genuine injection risk and must still fire.
    #[test]
    fn gated_still_flags_interpolated_sql_in_production() {
        let src = r#"await db.query(`SELECT * FROM users WHERE id = ${userId}`);"#;
        assert_eq!(
            crate::rules::test_helpers::run_rule_gated(&Check, src, "src/repo/users.ts").len(),
            1,
            "the rule must still fire on production paths"
        );
    }
}
