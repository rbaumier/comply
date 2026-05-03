//! no-typeof-undefined OxcCheck backend — flag `typeof x === 'undefined'`
//! when `x` is a property access (safe to rewrite to `x === undefined`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, UnaryOperator};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["typeof"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        // One side must be typeof, the other "undefined" string.
        let (typeof_arg, has_undefined_str) = match (&bin.left, &bin.right) {
            (Expression::UnaryExpression(unary), other) | (other, Expression::UnaryExpression(unary))
                if unary.operator == UnaryOperator::Typeof =>
            {
                let is_undef = is_undefined_string(other);
                (Some(&unary.argument), is_undef)
            }
            _ => (None, false),
        };

        let Some(arg) = typeof_arg else { return };
        if !has_undefined_str {
            return;
        }

        // Only flag when the operand is guaranteed to be a declared binding.
        let safe_to_rewrite = matches!(
            arg,
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_)
        );
        if !safe_to_rewrite {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `=== undefined` over `typeof … === 'undefined'` when \
                      the operand is a property access (which cannot throw \
                      ReferenceError)."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_undefined_string(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(lit) => lit.value == "undefined",
        _ => false,
    }
}
