//! OXC backend for ts-no-dynamic-delete.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_process_env(expr: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = expr else { return false };
    if member.property.name.as_str() != "env" {
        return false;
    }
    let Expression::Identifier(obj) = &member.object else { return false };
    obj.name.as_str() == "process"
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::UnaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::UnaryExpression(unary) = node.kind() else { return };
        if unary.operator != UnaryOperator::Delete {
            return;
        }

        // Argument must be a computed member expression: obj[expr]
        let Expression::ComputedMemberExpression(member) = &unary.argument else {
            return;
        };

        // Allow `delete process.env[key]` — only way to unset an env var in Node.js.
        if is_process_env(&member.object) {
            return;
        }

        // Allow literal string/number keys.
        match &member.expression {
            Expression::StringLiteral(_) | Expression::NumericLiteral(_) => return,
            // Allow negative number literals: `-42`
            Expression::UnaryExpression(inner)
                if inner.operator == UnaryOperator::UnaryNegation
                    && matches!(&inner.argument, Expression::NumericLiteral(_)) =>
            {
                return;
            }
            _ => {}
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.expression.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Do not delete dynamically computed property keys — use `Map` or `Set`."
                .into(),
            severity: Severity::Warning,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_dynamic_delete() {
        let diags = run_on("delete obj[key];");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_static_string_delete() {
        assert!(run_on(r#"delete obj["foo"];"#).is_empty());
    }

    #[test]
    fn allows_static_number_delete() {
        assert!(run_on("delete obj[42];").is_empty());
    }

    #[test]
    fn allows_dot_property_delete() {
        assert!(run_on("delete obj.foo;").is_empty());
    }

    // Regression #558 — process.env teardown in tests
    #[test]
    fn allows_delete_process_env_dynamic_key() {
        assert!(run_on("delete process.env[key];").is_empty());
    }

    #[test]
    fn allows_delete_process_env_string_literal_key() {
        assert!(run_on(r#"delete process.env['MY_VAR'];"#).is_empty());
    }

    #[test]
    fn still_flags_non_process_env_dynamic_delete() {
        let diags = run_on("delete obj[key];");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_process_env_teardown_pattern() {
        let src = r#"
const backup: Record<string, string | undefined> = {};
beforeEach(() => {
  backup['MY_VAR'] = process.env['MY_VAR'];
  delete process.env['MY_VAR'];
});
afterEach(() => {
  if (backup['MY_VAR'] === undefined) {
    delete process.env['MY_VAR'];
  } else {
    process.env['MY_VAR'] = backup['MY_VAR'];
  }
});
"#;
        assert!(run_on(src).is_empty());
    }
}
