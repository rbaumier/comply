//! drizzle-no-sql-raw-with-variable — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

/// Returns true when every template expression is wrapped in SQL double-quote
/// identifier syntax — `"${expr}"`. Such calls are safe DDL-identifier
/// patterns; bare `${expr}` interpolations remain flagged.
fn all_expressions_double_quoted(tpl: &oxc_ast::ast::TemplateLiteral) -> bool {
    for (i, _) in tpl.expressions.iter().enumerate() {
        let before = tpl.quasis[i].value.raw.as_str();
        let after = tpl.quasis[i + 1].value.raw.as_str();
        if !before.ends_with('"') || !after.starts_with('"') {
            return false;
        }
    }
    true
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["sql.raw"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be `sql.raw`.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "sql" || member.property.name.as_str() != "raw" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else { return };
        // String literal → safe.
        if matches!(first_arg, Argument::StringLiteral(_)) {
            return;
        }
        // Template literal → safe when no expressions (static string) or all
        // expressions are wrapped in SQL double-quote identifier syntax.
        if let Argument::TemplateLiteral(tpl) = first_arg {
            if all_expressions_double_quoted(tpl) {
                return;
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`sql.raw()` with a non-literal argument is a SQL injection vector — use `sql` tagged templates with parameterized values instead.".into(),
            severity: Severity::Error,
            span: None,
        });
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_variable_argument() {
        assert_eq!(run("sql.raw(userInput)").len(), 1);
    }

    #[test]
    fn flags_unquoted_template_substitution() {
        assert_eq!(run("sql.raw(`SELECT * FROM ${tableName}`)").len(), 1);
    }

    #[test]
    fn flags_mixed_quoted_and_unquoted() {
        assert_eq!(
            run(r#"sql.raw(`"${id}" WHERE col = ${value}`)"#).len(),
            1
        );
    }

    #[test]
    fn allows_string_literal() {
        assert!(run(r#"sql.raw("SELECT 1")"#).is_empty());
    }

    #[test]
    fn allows_static_template_literal() {
        assert!(run("sql.raw(`SELECT 1`)").is_empty());
    }

    /// Regression for issue #344: sql.raw with a DDL identifier from pg_class
    /// must not be flagged when the identifier is properly double-quoted.
    #[test]
    fn allows_double_quoted_identifier_in_template() {
        assert!(run(r#"sql.raw(`DROP INDEX IF EXISTS "${row.name}"`)"#).is_empty());
    }

    #[test]
    fn allows_multiple_double_quoted_identifiers() {
        assert!(run(r#"sql.raw(`ALTER TABLE "${schema}"."${table}" ADD COLUMN id int`)"#).is_empty());
    }
}
