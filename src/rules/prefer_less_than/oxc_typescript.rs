//! prefer-less-than oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

pub struct Check;

/// True when `expr` is a Yoda-style left operand: a literal value or a named
/// constant (SCREAMING_SNAKE_CASE identifier, possibly the final property of a
/// member access). For these, inverting `a > b` to `b < a` puts the variable
/// first and reads more naturally; otherwise `a > b` already reads naturally.
fn is_literal_or_constant_left(expr: &Expression) -> bool {
    match expr {
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::UnaryExpression(_) => true,
        Expression::Identifier(id) => super::is_screaming_snake_case(id.name.as_str()),
        // `Limits.MAX` — the final property determines constness.
        Expression::StaticMemberExpression(member) => {
            super::is_screaming_snake_case(member.property.name.as_str())
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else {
            return;
        };

        let suggested = match bin.operator {
            BinaryOperator::GreaterThan => "<",
            BinaryOperator::GreaterEqualThan => "<=",
            _ => return,
        };

        let op = match bin.operator {
            BinaryOperator::GreaterThan => ">",
            BinaryOperator::GreaterEqualThan => ">=",
            _ => return,
        };

        if !is_literal_or_constant_left(&bin.left) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Prefer `{suggested}` over `{op}` for readability — swap operands and use `{suggested}`."
            ),
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
    fn flags_literal_left() {
        let d = run_on("const r = 5 > x;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<`"));
    }

    #[test]
    fn flags_literal_left_greater_or_equal() {
        let d = run_on("const r = 5 >= x;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<=`"));
    }

    #[test]
    fn flags_constant_left() {
        let d = run_on("const r = MAX > x;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_member_constant_left() {
        let d = run_on("const r = Limits.MAX_DIFF_LINES > count;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_variable_vs_literal() {
        assert!(run_on("if (x > 0) { f(); }").is_empty());
        assert!(run_on("if (arr.length >= 1) { f(); }").is_empty());
        assert!(run_on("const ok = count > 5;").is_empty());
    }

    #[test]
    fn allows_method_call_vs_constant() {
        assert!(run_on("const r = doc.lenLines() > MAX_DIFF_LINES;").is_empty());
    }

    #[test]
    fn allows_non_constant_identifier_left() {
        assert!(run_on("const r = a > b;").is_empty());
        assert!(run_on("const r = b > a;").is_empty());
    }

    #[test]
    fn allows_less_than() {
        assert!(run_on("const r = a < b;").is_empty());
    }

    #[test]
    fn allows_less_or_equal() {
        assert!(run_on("const r = a <= b;").is_empty());
    }

    #[test]
    fn allows_equality() {
        assert!(run_on("const r = a === b;").is_empty());
    }
}
