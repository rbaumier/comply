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

use super::position::all_substitutions_in_identifier_position;

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
                let fragments: Vec<&str> =
                    tpl.quasis.iter().map(|q| q.value.raw.as_str()).collect();
                let static_text = fragments.join(" ");
                if !is_sql_string(&static_text) {
                    return;
                }
                if static_text.contains("$1") || static_text.contains("$2") {
                    return;
                }
                // Every interpolation sits in an identifier position (a relation
                // or column name): those cannot be bind parameters, so this is
                // the only possible form, not an injection.
                if all_substitutions_in_identifier_position(&fragments) {
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
                // When the SQL string is the left operand, the dynamic right
                // operand is appended at its end. If that end is an identifier
                // position (`"... FROM " + table`), the value names a relation
                // and cannot be a bind parameter, so it is not an injection.
                if left_sql
                    && let Some(prefix) = string_expr_value(&bin.left)
                    && all_substitutions_in_identifier_position(&[&prefix, ""])
                {
                    return;
                }
                // Skip parameterised queries.
                let start = bin.span.start as usize;
                let end = bin.span.end as usize;
                if let Some(combined) = ctx.source.get(start..end)
                    && (combined.contains("$1") || combined.contains("$2")) {
                        return;
                    }
                // Skip diagnostic strings: a concatenation consumed by an
                // error constructor (`new Error(...)`) or a `console.*` call
                // is a message, never a query.
                if concat_feeds_diagnostic_sink(node, semantic) {
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

/// The static text of a string-literal or interpolation-free template-literal
/// expression, for inspecting what precedes an appended concat operand. Returns
/// `None` for a template literal that itself interpolates (its trailing text is
/// not a single static string the position check can key off).
fn string_expr_value(expr: &Expression) -> Option<String> {
    match expr.without_parentheses() {
        Expression::StringLiteral(lit) => Some(lit.value.to_string()),
        Expression::TemplateLiteral(tpl) if tpl.expressions.is_empty() => Some(
            tpl.quasis
                .iter()
                .map(|q| q.value.raw.as_str())
                .collect::<String>(),
        ),
        _ => None,
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

/// True when the flagged `+`-concatenation is consumed by a diagnostic
/// sink — an error constructor (`new Error(...)`, `new ValidationError(...)`,
/// …) or a `console.*` call. These build messages, never SQL queries, so a
/// SQL-keyword match in them is a false positive.
///
/// Walks up from the concat node through the enclosing `+`-chain and
/// parentheses to the nearest consuming expression, then inspects only that
/// immediate consumer — an Error `new` elsewhere in the function does not
/// exempt an unrelated query string.
fn concat_feeds_diagnostic_sink(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            // Still inside the concatenation: keep climbing to the consumer.
            AstKind::ParenthesizedExpression(_) => continue,
            AstKind::BinaryExpression(bin)
                if bin.operator == oxc_ast::ast::BinaryOperator::Addition =>
            {
                continue;
            }
            AstKind::NewExpression(new_expr) => {
                return callee_is_error_constructor(&new_expr.callee);
            }
            AstKind::CallExpression(call) => {
                return callee_is_console_method(&call.callee);
            }
            // Any other consumer (assignment, return, query call, …): not a
            // diagnostic sink, so the SQL-injection finding stands.
            _ => return false,
        }
    }
    false
}

/// True when `callee` is an `Error`-like constructor: a known built-in
/// (`Error`, `TypeError`, …) or — following the ubiquitous convention for
/// custom error classes — any identifier whose name ends in `Error`.
fn callee_is_error_constructor(callee: &Expression) -> bool {
    matches!(
        callee.without_parentheses(),
        Expression::Identifier(ident) if ident.name.ends_with("Error")
    )
}

/// True when `callee` is a `console.<method>(...)` member access for one of
/// the standard logging methods.
fn callee_is_console_method(callee: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = callee.without_parentheses() else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    obj.name == "console"
        && matches!(
            member.property.name.as_str(),
            "log" | "warn" | "error" | "info" | "debug" | "trace"
        )
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

    // Regression for rbaumier/comply#3358 — a Prisma client code generator
    // emits JSDoc template strings containing Prisma API method names
    // (`update`, `where`, `data`) that mirror SQL verbs. With interpolation
    // but no SQL clause structure (UPDATE needs SET), this is generated
    // documentation, not a query.
    #[test]
    fn does_not_flag_prisma_jsdoc_codegen_template() {
        let src = r#"
            const jsdoc = {
              update: {
                body: (ctx) =>
                  `Update one ${ctx.singular}.
            @param {${getModelArgName(ctx.model.name, ctx.action)}} args - Arguments to update one ${ctx.singular}.
            @example
            // Update one ${ctx.singular}
            const ${uncapitalize(ctx.mapping.model)} = await ${ctx.method}({
              where: {
                // ... provide filter here
              },
              data: {
                // ... provide data here
              }
            })`,
              },
            };
        "#;
        assert!(run_on(src).is_empty());
    }

    // Regression for #3358 — CLI help text describing a migrate command.
    // "Update the database schema" pairs `update` with no SET clause.
    #[test]
    fn does_not_flag_cli_help_template() {
        let src = r#"
            const help = format(`
            Update the database schema with migrations

            Usage
              $ prisma migrate [command] [options]
            `);
        "#;
        assert!(run_on(src).is_empty());
    }

    // Regression for #3358 — log message: "Would update X from Y" pairs the
    // word `update` with the English preposition `from`, never SET/FROM in
    // clause order. Not a SQL statement.
    #[test]
    fn does_not_flag_log_message_template() {
        let src = r#"
            console.log(`Would update ${pkgJsonPath} from ${packageJson.version} to ${version} now`);
        "#;
        assert!(run_on(src).is_empty());
    }

    // Regression for #3312 — an error message built by concatenating string
    // operands that coincidentally contain SQL-shaped wording ("select
    // from …") is a diagnostic string, not a query. The concat is consumed
    // by `new Error(...)`, so it must not fire.
    #[test]
    fn does_not_flag_error_constructor_message_concat() {
        let src = r#"throw new Error(`select from the registry` + ` was removed`);"#;
        assert!(run_on(src).is_empty());
    }

    // #3312 — same for a `console.*` call: the concatenation feeds a log
    // method, never a database method.
    #[test]
    fn does_not_flag_console_call_message_concat() {
        let src = r#"console.error("select from " + table + " were deleted");"#;
        assert!(run_on(src).is_empty());
    }

    // #3312 — custom error classes follow the `*Error` naming convention;
    // `new ValidationError(...)` is an error constructor too.
    #[test]
    fn does_not_flag_custom_error_constructor_message_concat() {
        let src = r#"throw new ValidationError("select from " + field);"#;
        assert!(run_on(src).is_empty());
    }

    // #3312 guard — a genuine query string concatenated into a variable (not
    // an Error/console argument) is still an injection risk and must fire.
    #[test]
    fn still_flags_query_concat_assigned_to_variable() {
        let src = r#"const q = "SELECT * FROM t WHERE x = " + v;"#;
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
