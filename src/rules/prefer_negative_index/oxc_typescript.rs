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

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_slice_length_minus() {
        let d = run_on("const x = str.slice(str.length - 3);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_splice_length_minus() {
        let d = run_on("arr.splice(arr.length - 1, 1);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_at_length_minus() {
        let d = run_on("const last = arr.at(arr.length - 1);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_negative_index() {
        assert!(run_on("const x = str.slice(-3);").is_empty());
    }


    #[test]
    fn allows_different_receiver() {
        assert!(run_on("const x = str.slice(other.length - 3);").is_empty());
    }


    #[test]
    fn allows_normal_slice() {
        assert!(run_on("const x = str.slice(0, 5);").is_empty());
    }
}
