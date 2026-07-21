//! prefer-less-than oxc backend — flag `>` / `>=` comparisons whose left operand
//! is strictly more constant-like than the right, and suggest the swapped
//! `<` / `<=` form. When the right operand is at least as constant-like
//! (`MAX > 0`, `a > b`), swapping would create a Yoda condition rather than
//! remove one.

use super::Constness;
use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, peel_parens};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

pub struct Check;

/// Rank an oxc operand expression on the `Constness` scale.
fn constness(expr: &Expression) -> Constness {
    // `peel_parens` makes `MAX > (0)` rank like `MAX > 0`.
    match peel_parens(expr) {
        Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => Constness::Literal,
        // Unary `-` / `!` keeps the value-ness of its argument, so `-1` ranks as
        // a literal while `-x` and `typeof x` stay subjects.
        Expression::UnaryExpression(unary) => constness(&unary.argument),
        Expression::Identifier(id) => super::name_constness(id.name.as_str()),
        // `Limits.MAX` — the final property determines constness.
        Expression::StaticMemberExpression(member) => {
            super::name_constness(member.property.name.as_str())
        }
        _ => Constness::Subject,
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

        if constness(&bin.left) <= constness(&bin.right) {
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_literal_left() {
        let d = run_on("const r = 5 > x;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<`"));
        assert_eq!(run_on("const r = (5) > x;").len(), 1);
        assert_eq!(run_on("const r = 100n > count;").len(), 1);
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

    // The TS spelling of the constant-vs-literal case — see `Constness`.
    #[test]
    fn allows_constant_vs_literal() {
        assert!(run_on("const r = MAX > 0;").is_empty());
        assert!(run_on("const r = Limits.MAX_DIFF_LINES >= 1;").is_empty());
        assert!(run_on("const r = MAX > 100n;").is_empty());
        assert!(run_on("const r = MAX > (0);").is_empty());
    }

    // Equal ranks on both sides: the strict rank test rejects either direction.
    #[test]
    fn allows_equal_rank_operands() {
        assert!(run_on("const r = 5 > 3;").is_empty());
        assert!(run_on("const r = MAX > MIN;").is_empty());
    }

    #[test]
    fn allows_negated_variable_vs_literal() {
        assert!(run_on("const r = -x > 5;").is_empty());
        assert!(run_on("const r = typeof x > 5;").is_empty());
    }

    #[test]
    fn flags_negative_literal_left() {
        assert_eq!(run_on("const r = -1 > x;").len(), 1);
    }

    #[test]
    fn flags_literal_vs_constant() {
        assert_eq!(run_on("const r = 5 > MAX;").len(), 1);
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
