//! redundant-logical-operand OXC backend.
//!
//! Flags logical expressions whose result is fixed by a literal operand:
//! a boolean literal on either side of `&&` / `||`, or a `null` literal on
//! the left of `??`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, LogicalExpression, LogicalOperator, UnaryOperator};
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

/// The simplification message for a logical expression, or `None` when no
/// operand is a redundant literal.
fn redundant_message(logical: &LogicalExpression) -> Option<&'static str> {
    match logical.operator {
        LogicalOperator::And => match (bool_literal(&logical.left), bool_literal(&logical.right)) {
            (Some(true), _) => Some("`true && x` is just `x` — drop the redundant `true`."),
            (Some(false), _) => Some("`false && x` is always `false` — drop the redundant operand."),
            // `x && true` returns `x` (its original type), so it equals `x`
            // only when `x` is already a boolean — otherwise `&& true` is a
            // meaningful coercion to a strict `boolean`. Flag by shape only.
            (_, Some(true)) if is_provably_boolean(&logical.left) => {
                Some("`x && true` is just `x` — drop the redundant `true`.")
            }
            (_, Some(false)) => Some("`x && false` is always `false` — drop the redundant operand."),
            _ => None,
        },
        LogicalOperator::Or => match (bool_literal(&logical.left), bool_literal(&logical.right)) {
            (Some(true), _) => Some("`true || x` is always `true` — drop the redundant operand."),
            (Some(false), _) => Some("`false || x` is just `x` — drop the redundant `false`."),
            (_, Some(true)) => Some("`x || true` is always `true` — drop the redundant operand."),
            // `x || false` returns `x` (its original type), so it equals `x`
            // only when `x` is already a boolean — otherwise `|| false` is a
            // meaningful coercion to a strict `boolean`. Flag by shape only.
            (_, Some(false)) if is_provably_boolean(&logical.left) => {
                Some("`x || false` is just `x` — drop the redundant `false`.")
            }
            _ => None,
        },
        LogicalOperator::Coalesce => {
            if matches!(logical.left, Expression::NullLiteral(_)) {
                Some("`null ?? x` is just `x` — drop the redundant `null`.")
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
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::LogicalExpression(logical) = node.kind() else {
            return;
        };
        let Some(message) = redundant_message(logical) else {
            return;
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, logical.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: message.into(),
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
