//! OxcCheck backend for no-useless-fallback-in-spread.
//!
//! Flags `{...(foo || {})}` and `{...(foo ?? {})}`. Spreading
//! `undefined`/`null` in an object literal is already a no-op, so
//! the fallback is unnecessary.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, LogicalOperator, ObjectPropertyKind};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectExpression(obj) = node.kind() else {
            return;
        };

        for prop in &obj.properties {
            let ObjectPropertyKind::SpreadProperty(spread) = prop else {
                continue;
            };

            // Unwrap parenthesized expression
            let inner = match &spread.argument {
                Expression::ParenthesizedExpression(paren) => &paren.expression,
                other => other,
            };

            let Expression::LogicalExpression(logical) = inner else {
                continue;
            };

            // Must be `||` or `??`
            let op = match logical.operator {
                LogicalOperator::Or => "||",
                LogicalOperator::Coalesce => "??",
                _ => continue,
            };

            // Right side must be an empty object literal
            let Expression::ObjectExpression(right_obj) = &logical.right else {
                continue;
            };
            if !right_obj.properties.is_empty() {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, spread.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "The `{op} {{}}` fallback is unnecessary — spreading \
                     `undefined`/`null` in an object literal is a no-op."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_or_empty_object() {
        assert_eq!(run_on("const x = {...(foo || {})};").len(), 1);
    }

    #[test]
    fn flags_nullish_coalescing_empty_object() {
        assert_eq!(run_on("const x = {...(foo ?? {})};").len(), 1);
    }

    #[test]
    fn allows_spread_variable() {
        assert!(run_on("const x = {...foo};").is_empty());
    }

    #[test]
    fn allows_non_empty_fallback() {
        assert!(run_on("const x = {...(foo || {a: 1})};").is_empty());
    }

    #[test]
    fn allows_or_in_non_spread_context() {
        assert!(run_on("const x = foo || {};").is_empty());
    }

    #[test]
    fn allows_spread_in_array() {
        assert!(run_on("const x = [...(foo || [])];").is_empty());
    }
}
