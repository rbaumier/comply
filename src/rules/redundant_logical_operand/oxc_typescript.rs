//! redundant-logical-operand OXC backend.
//!
//! Flags logical expressions whose result is fixed by a literal operand:
//! a boolean literal on either side of `&&` / `||`, or a `null` literal on
//! the left of `??`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, LogicalExpression, LogicalOperator, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn bool_literal(expr: &Expression) -> Option<bool> {
    if let Expression::BooleanLiteral(lit) = expr {
        Some(lit.value)
    } else {
        None
    }
}

/// Names of builtin methods/functions whose return type is always `boolean`,
/// regardless of the receiver — used to recognize a boolean-valued call by
/// shape alone (no type information).
const BOOL_RETURNING_METHODS: &[&str] = &[
    "includes",
    "some",
    "every",
    "has",
    "test",
    "startsWith",
    "endsWith",
    "isArray",
    "isInteger",
    "isNaN",
];

/// Whether a call expression is a boolean-returning builtin by callee shape:
/// a member call to a known boolean method (`x.includes(y)`), or a global
/// `Boolean(...)` / `Array.isArray(...)` / `Number.isInteger(...)` /
/// `Number.isNaN(...)`.
fn is_boolean_call(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(ident) => ident.name.as_str() == "Boolean",
        Expression::StaticMemberExpression(member) => {
            BOOL_RETURNING_METHODS.contains(&member.property.name.as_str())
        }
        _ => false,
    }
}

/// Whether `expr` is provably a `boolean` value by its syntactic shape alone,
/// with no type information. Conservative: returns `false` whenever the value
/// type cannot be proven from the shape (bare identifiers, member access,
/// unknown calls), so the redundancy rule errs toward a missed true-positive
/// rather than a false positive.
fn is_provably_boolean(expr: &Expression) -> bool {
    match expr {
        Expression::ParenthesizedExpression(paren) => is_provably_boolean(&paren.expression),
        Expression::BooleanLiteral(_) => true,
        Expression::BinaryExpression(bin) => {
            bin.operator.is_equality() || bin.operator.is_compare() || bin.operator.is_relational()
        }
        Expression::UnaryExpression(unary) => unary.operator == UnaryOperator::LogicalNot,
        Expression::CallExpression(call) => is_boolean_call(call),
        Expression::LogicalExpression(logical) => {
            is_provably_boolean(&logical.left) && is_provably_boolean(&logical.right)
        }
        _ => false,
    }
}

/// A redundant-operand finding: the simplification message plus whether it is
/// sound only in a boolean-coercion context.
struct Redundant {
    message: &'static str,
    /// `x || true` / `x && false` short-circuit to their *left* operand when it
    /// is truthy/falsy, so the "always true/false" claim (and the drop-the-
    /// operand fix) holds only when the value is coerced to a boolean. In a
    /// value position the left operand is load-bearing — set this so `run`
    /// suppresses the finding unless the expression sits in a boolean context.
    boolean_context_only: bool,
}

/// Whether `node`'s result is ultimately consumed as a boolean: the `test` of an
/// `if`/`while`/`do-while`/`for`/ternary (matched by span to distinguish the
/// test from a branch or loop body), the operand of a `!`, or an operand of an
/// enclosing logical expression that is itself in a boolean context. A
/// `ParenthesizedExpression` is transparent. Anything else is a value position.
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

/// The simplification message for a logical expression, or `None` when no
/// operand is a redundant literal.
fn redundant_message(logical: &LogicalExpression) -> Option<Redundant> {
    let message = |message| Some(Redundant { message, boolean_context_only: false });
    match logical.operator {
        LogicalOperator::And => match (bool_literal(&logical.left), bool_literal(&logical.right)) {
            (Some(true), _) => message("`true && x` is just `x` — drop the redundant `true`."),
            (Some(false), _) => message("`false && x` is always `false` — drop the redundant operand."),
            // `x && true` returns `x` (its original type), so it equals `x`
            // only when `x` is already a boolean — otherwise `&& true` is a
            // meaningful coercion to a strict `boolean`. Flag by shape only.
            (_, Some(true)) if is_provably_boolean(&logical.left) => {
                message("`x && true` is just `x` — drop the redundant `true`.")
            }
            (_, Some(false)) => Some(Redundant {
                message: "`x && false` is always `false` — drop the redundant operand.",
                boolean_context_only: true,
            }),
            _ => None,
        },
        LogicalOperator::Or => match (bool_literal(&logical.left), bool_literal(&logical.right)) {
            (Some(true), _) => message("`true || x` is always `true` — drop the redundant operand."),
            (Some(false), _) => message("`false || x` is just `x` — drop the redundant `false`."),
            (_, Some(true)) => Some(Redundant {
                message: "`x || true` is always `true` — drop the redundant operand.",
                boolean_context_only: true,
            }),
            // `x || false` returns `x` (its original type), so it equals `x`
            // only when `x` is already a boolean — otherwise `|| false` is a
            // meaningful coercion to a strict `boolean`. Flag by shape only.
            (_, Some(false)) if is_provably_boolean(&logical.left) => {
                message("`x || false` is just `x` — drop the redundant `false`.")
            }
            _ => None,
        },
        LogicalOperator::Coalesce => {
            if matches!(logical.left, Expression::NullLiteral(_)) {
                message("`null ?? x` is just `x` — drop the redundant `null`.")
            } else {
                None
            }
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::LogicalExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["&&", "||", "??"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::LogicalExpression(logical) = node.kind() else {
            return;
        };
        let Some(redundant) = redundant_message(logical) else {
            return;
        };
        if redundant.boolean_context_only && !is_consumed_as_boolean(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, logical.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: redundant.message.into(),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // Regression for #7219: `x || true` / `x && false` in a value position
    // short-circuits to the load-bearing left operand (yields `x` when
    // truthy/falsy), so the "always true/false" claim is unsound — must not be
    // flagged.
    #[test]
    fn allows_or_true_in_call_argument() {
        // prisma/prisma applyFluent.ts: `deepSet(...)` receives `callArgs` when
        // present, or the sentinel `true` otherwise — `UserArgs | true`.
        assert!(run(r#"deepSet(prevArgs, nextDataPath, callArgs || true);"#).is_empty());
    }

    #[test]
    fn allows_or_true_in_assignment_value_position() {
        assert!(run(r#"const y = x || true;"#).is_empty());
    }

    #[test]
    fn allows_and_false_in_return_value_position() {
        assert!(run(r#"function f(x: unknown) { return x && false; }"#).is_empty());
    }

    // True positives: in a boolean context the short-circuit IS redundant.
    #[test]
    fn flags_or_true_in_if_test() {
        assert_eq!(run(r#"if (x || true) {}"#).len(), 1);
    }

    #[test]
    fn flags_and_false_in_while_test() {
        assert_eq!(run(r#"while (x && false) {}"#).len(), 1);
    }

    #[test]
    fn flags_or_true_under_negation() {
        assert_eq!(run(r#"if (!(x || true)) {}"#).len(), 1);
    }

    #[test]
    fn flags_or_true_as_logical_operand_in_boolean_context() {
        assert_eq!(run(r#"if (a && (x || true)) {}"#).len(), 1);
    }

    // The already-gated mirror cases are unchanged: `x && true` / `x || false`
    // flag only when the left operand is provably boolean, in any position.
    #[test]
    fn flags_and_true_when_left_provably_boolean() {
        assert_eq!(run(r#"const b = (a === c) && true;"#).len(), 1);
    }

    #[test]
    fn allows_or_false_when_left_not_provably_boolean() {
        assert!(run(r#"const v = x || false;"#).is_empty());
    }

    // Unconditionally-true literal-left cases stay flagged everywhere.
    #[test]
    fn flags_true_or_x_in_value_position() {
        assert_eq!(run(r#"const v = true || x;"#).len(), 1);
    }
}
