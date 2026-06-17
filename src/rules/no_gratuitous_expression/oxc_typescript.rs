//! no-gratuitous-expression OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, LogicalOperator, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Is the result of `node` ultimately consumed as a boolean?
///
/// `X || true` short-circuits to `X` when `X` is truthy and only to `true`
/// otherwise — so the "always true" claim (and the "remove the dead branch"
/// remediation) is sound *only* when the value is coerced to a boolean. In a
/// value position (`const x = foo || true`, a JSX prop, a `return`, a ternary
/// branch) the expression is a deliberate coerce-to-truthy-while-preserving-content
/// idiom and must not be flagged.
///
/// Walks the parent chain: a `ParenthesizedExpression` is transparent; an
/// operand of an enclosing `LogicalExpression` inherits that logical's context
/// (recurse on the parent); `!x` is boolean; the `test` of an
/// `if`/`while`/`do-while`/`for`/ternary is boolean (matched by span, to
/// distinguish the test from a ternary branch or a loop body). Anything else is
/// a value position.
fn is_consumed_as_boolean(node: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    // Root node's parent is itself; stop to avoid infinite recursion.
    if parent.id() == node.id() {
        return false;
    }
    let node_span = node.span();
    match parent.kind() {
        AstKind::ParenthesizedExpression(_) => is_consumed_as_boolean(parent, semantic),
        AstKind::LogicalExpression(_) => is_consumed_as_boolean(parent, semantic),
        AstKind::UnaryExpression(unary) => unary.operator == UnaryOperator::LogicalNot,
        AstKind::IfStatement(s) => s.test.span() == node_span,
        AstKind::WhileStatement(s) => s.test.span() == node_span,
        AstKind::DoWhileStatement(s) => s.test.span() == node_span,
        AstKind::ForStatement(s) => s.test.as_ref().is_some_and(|t| t.span() == node_span),
        AstKind::ConditionalExpression(s) => s.test.span() == node_span,
        _ => false,
    }
}

fn detect_self_comparison(op: BinaryOperator, left: &Expression, right: &Expression, source: &str) -> Option<&'static str> {
    // Both sides must be identifiers (or member expressions) with the same text
    let left_span = left.span();
    let right_span = right.span();
    let left_text = &source[left_span.start as usize..left_span.end as usize];
    let right_text = &source[right_span.start as usize..right_span.end as usize];

    let left_trimmed = left_text.trim();
    let right_trimmed = right_text.trim();

    if left_trimmed.is_empty() || left_trimmed != right_trimmed {
        return None;
    }
    if !left_trimmed.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
        return None;
    }

    match op {
        // `x !== x` / `x != x` is the canonical NaN-detection idiom: `NaN` is the
        // only value not equal to itself, so an inequality of identical operands is
        // a deliberate test, not a dead branch. ESLint's `no-self-compare`
        // documents this same exception. Equality self-comparison stays flagged —
        // `x === x` is genuinely always true.
        BinaryOperator::StrictInequality | BinaryOperator::Inequality => None,
        BinaryOperator::StrictEquality | BinaryOperator::Equality => {
            Some("comparison `x === x` is always true (unless NaN)")
        }
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement, AstType::BinaryExpression, AstType::LogicalExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["true", "false"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::IfStatement(if_stmt) => {
                if let Expression::BooleanLiteral(lit) = &if_stmt.test {
                    let msg = if lit.value {
                        "Gratuitous expression: condition is always true."
                    } else {
                        "Gratuitous expression: condition is always false."
                    };
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, if_stmt.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: msg.into(),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            AstKind::LogicalExpression(logical) => {
                // `&& false` → always false; `|| true` → always true — but only in
                // a boolean context. In a value position `X || true` is the
                // coerce-to-truthy-while-preserving-content idiom (yields `X` when
                // truthy), so the "always true/false" claim is unsound there.
                if !is_consumed_as_boolean(node, semantic) {
                    return;
                }
                match logical.operator {
                    LogicalOperator::And => {
                        if let Expression::BooleanLiteral(lit) = &logical.right
                            && !lit.value {
                                let (line, column) =
                                    byte_offset_to_line_col(ctx.source, logical.span.start as usize);
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line,
                                    column,
                                    rule_id: super::META.id.into(),
                                    message: "Gratuitous expression: expression is always false (short-circuited by `&& false`).".into(),
                                    severity: Severity::Error,
                                    span: None,
                                });
                            }
                    }
                    LogicalOperator::Or => {
                        if let Expression::BooleanLiteral(lit) = &logical.right
                            && lit.value {
                                let (line, column) =
                                    byte_offset_to_line_col(ctx.source, logical.span.start as usize);
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line,
                                    column,
                                    rule_id: super::META.id.into(),
                                    message: "Gratuitous expression: expression is always true (short-circuited by `|| true`).".into(),
                                    severity: Severity::Error,
                                    span: None,
                                });
                            }
                    }
                    _ => {}
                }
            }
            AstKind::BinaryExpression(bin) => {
                if let Some(message) = detect_self_comparison(bin.operator, &bin.left, &bin.right, ctx.source) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, bin.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!("Gratuitous expression: {}.", message),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            _ => {}
        }
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // Regression for #1894: `x !== x` is the canonical NaN-detection idiom, not
    // a dead branch. `NaN` is the only value not equal to itself.
    #[test]
    fn allows_strict_inequality_nan_idiom() {
        let src = r#"export const isNaN = (obj: any): boolean => obj !== obj;"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_loose_inequality_nan_idiom() {
        assert!(run(r#"const isNaN = (x: any) => x != x;"#).is_empty());
    }

    #[test]
    fn flags_strict_equality_self_compare() {
        assert_eq!(run(r#"const b = x === x;"#).len(), 1);
    }

    #[test]
    fn flags_loose_equality_self_compare() {
        assert_eq!(run(r#"const b = x == x;"#).len(), 1);
    }

    // Regression for #3932: `X || true` / `X && false` in a value position is the
    // coerce-to-truthy-while-preserving-content idiom (yields `X` when truthy),
    // not a dead branch — must not be flagged.
    #[test]
    fn allows_or_true_in_assignment_value_position() {
        assert!(run(r#"const x = foo || true;"#).is_empty());
    }

    #[test]
    fn allows_or_true_in_call_argument() {
        assert!(run(r#"f(validationError || true);"#).is_empty());
    }

    #[test]
    fn allows_or_true_in_ternary_branch() {
        // The mantine case: `error={valid ? error : validationError || true}` —
        // the logical sits in the ternary's alternate (a value branch), not the test.
        assert!(run(r#"const e = valid ? error : validationError || true;"#).is_empty());
    }

    #[test]
    fn allows_and_false_in_value_position() {
        assert!(run(r#"const y = bar && false;"#).is_empty());
    }

    // True positives: in a boolean context the short-circuit IS gratuitous.
    #[test]
    fn flags_or_true_in_if_test() {
        assert_eq!(run(r#"if (foo || true) {}"#).len(), 1);
    }

    #[test]
    fn flags_and_false_in_while_test() {
        assert_eq!(run(r#"while (bar && false) {}"#).len(), 1);
    }

    #[test]
    fn flags_or_true_under_negation() {
        assert_eq!(run(r#"if (!(foo || true)) {}"#).len(), 1);
    }

    #[test]
    fn flags_or_true_as_ternary_test() {
        assert_eq!(run(r#"const z = (foo || true) ? a : b;"#).len(), 1);
    }
}
