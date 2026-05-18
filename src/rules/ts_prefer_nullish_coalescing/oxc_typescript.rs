//! ts-prefer-nullish-coalescing oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, LogicalOperator, UnaryOperator};
use std::sync::Arc;

pub struct Check;

/// Methods whose return type is reliably `boolean`. Used to recognise a
/// boolean-producing call without full type inference.
const BOOLEAN_METHODS: &[&str] = &[
    "endsWith",
    "startsWith",
    "includes",
    "has",
    "isArray",
    "isInteger",
    "isFinite",
    "isNaN",
    "isSafeInteger",
    "test",
    "equals",
];

/// Syntactic heuristic: is `expr` very likely to evaluate to a boolean?
/// Conservative — only patterns whose result is *always* boolean qualify,
/// so that we never silence a legitimate `x || "default"` warning.
fn looks_boolean(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::BooleanLiteral(_) => true,
        Expression::UnaryExpression(u) => u.operator == UnaryOperator::LogicalNot,
        Expression::BinaryExpression(b) => matches!(
            b.operator,
            BinaryOperator::Equality
                | BinaryOperator::Inequality
                | BinaryOperator::StrictEquality
                | BinaryOperator::StrictInequality
                | BinaryOperator::LessThan
                | BinaryOperator::GreaterThan
                | BinaryOperator::LessEqualThan
                | BinaryOperator::GreaterEqualThan
                | BinaryOperator::In
                | BinaryOperator::Instanceof
        ),
        Expression::LogicalExpression(log) => {
            matches!(log.operator, LogicalOperator::And | LogicalOperator::Or)
                && looks_boolean(&log.left)
                && looks_boolean(&log.right)
        }
        Expression::CallExpression(call) => {
            if let Expression::StaticMemberExpression(member) = &call.callee {
                BOOLEAN_METHODS.contains(&member.property.name.as_str())
            } else {
                false
            }
        }
        _ => false,
    }
}

/// True if `expr` is a literal that's NOT null/undefined — the `||`
/// shape `foo || "default"` is the canonical case we want to flag.
/// Skip when the RHS is a boolean literal (those usually intentionally
/// short-circuit on any falsy LHS) or a numeric `0`/`1` (likely
/// arithmetic identity).
fn rhs_is_default_like(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => true,
        Expression::ArrayExpression(_) | Expression::ObjectExpression(_) => true,
        Expression::NumericLiteral(n) => n.value != 0.0 && n.value != 1.0,
        Expression::Identifier(_) => true,
        Expression::CallExpression(_) => true,
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::LogicalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::LogicalExpression(logical) = node.kind() else {
            return;
        };
        if logical.operator != LogicalOperator::Or {
            return;
        }
        if !rhs_is_default_like(&logical.right) {
            return;
        }
        // Boolean `||` chains (`a.endsWith(":asc") || a.endsWith(":desc")`,
        // `flag || isReady()`) are an intentional disjunction, not a
        // nullish fallback — both sides already evaluate to `boolean`.
        if looks_boolean(&logical.left) && looks_boolean(&logical.right) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, logical.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`||` triggers on every falsy value (0, \"\", false). For a \
                      nullish fallback, use `??` so legitimate falsy values pass \
                      through."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_string_default() {
        let src = r#"const x = name || "anonymous";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_boolean_endswith_chain() {
        // Issue #111 reproducer.
        let src = r#"function f(candidate: string) {
            return candidate.endsWith(":asc") || candidate.endsWith(":desc");
        }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_boolean_comparison_chain() {
        let src = r#"const ok = x > 0 || y < 10;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_negation_chain() {
        let src = r#"const ok = !a || !b;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_boolean_literal_chain() {
        let src = r#"const ok = isReady || false;"#;
        // RHS is a BooleanLiteral so it isn't default-like — already skipped.
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_includes_chain() {
        let src = r#"const ok = list.includes(a) || list.includes(b);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_array_isarray_chain() {
        let src = r#"const ok = Array.isArray(x) || Array.isArray(y);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mixed_boolean_logical_chain() {
        let src = r#"const ok = (a > 0 && b < 5) || c.startsWith("x");"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_mixed_unknown_lhs() {
        // LHS isn't syntactically boolean → still warns.
        let src = r#"const x = maybeStr || "default";"#;
        assert_eq!(run(src).len(), 1);
    }
}
