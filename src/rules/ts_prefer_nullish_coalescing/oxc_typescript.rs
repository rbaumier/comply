//! ts-prefer-nullish-coalescing oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, LogicalOperator};
use std::sync::Arc;

pub struct Check;

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
