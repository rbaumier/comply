//! OxcCheck backend for prefer-negative-index.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const METHODS: &[&str] = &["slice", "splice", "toSpliced", "at", "with", "subarray"];

/// Check if an expression is `<receiver>.length - <expr>`.
fn is_length_minus<'a>(expr: &Expression<'a>, source: &str, receiver_text: &str) -> bool {
    let Expression::BinaryExpression(bin) = expr else { return false };
    if bin.operator != BinaryOperator::Subtraction {
        return false;
    }
    let Expression::StaticMemberExpression(member) = &bin.left else { return false };
    if member.property.name.as_str() != "length" {
        return false;
    }
    let obj_span = member.object.span();
    let obj_text = &source[obj_span.start as usize..obj_span.end as usize];
    obj_text == receiver_text
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method_name = member.property.name.as_str();
        if !METHODS.contains(&method_name) {
            return;
        }

        let obj_span = member.object.span();
        let receiver = &ctx.source[obj_span.start as usize..obj_span.end as usize];
        if receiver.is_empty() {
            return;
        }

        for arg in &call.arguments {
            let Some(expr) = arg.as_expression() else { continue };
            if is_length_minus(expr, ctx.source, receiver) {
                let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer negative index over `.length - index`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                return; // one diagnostic per call
            }
        }
    }
}
