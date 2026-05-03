//! prefer-array-some oxc backend — flag `.filter(...).length > 0` etc.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

pub struct Check;

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
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        // Check operator: > 0, !== 0, != 0, >= 1
        let right_val = match &bin.right {
            Expression::NumericLiteral(n) => n.value,
            _ => return,
        };

        let is_existence_check = match bin.operator {
            BinaryOperator::GreaterThan => right_val == 0.0,
            BinaryOperator::StrictInequality | BinaryOperator::Inequality => right_val == 0.0,
            BinaryOperator::GreaterEqualThan => right_val == 1.0,
            _ => false,
        };
        if !is_existence_check {
            return;
        }

        // Left side: `<expr>.length` where `<expr>` is `.filter(...)`.
        let Expression::StaticMemberExpression(length_member) = &bin.left else { return };
        if length_member.property.name.as_str() != "length" {
            return;
        }

        let Expression::CallExpression(filter_call) = &length_member.object else { return };
        let Expression::StaticMemberExpression(filter_member) = &filter_call.callee else {
            return;
        };
        if filter_member.property.name.as_str() != "filter" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `.some(\u{2026})` over `.filter(\u{2026}).length` check \u{2014} `.some()` short-circuits.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
