//! no-identical-expressions oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BinaryOperator;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression, AstType::LogicalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::BinaryExpression(bin) => {
                let op_str = match bin.operator {
                    BinaryOperator::StrictEquality => "===",
                    BinaryOperator::StrictInequality => "!==",
                    BinaryOperator::Subtraction => "-",
                    BinaryOperator::Division => "/",
                    _ => return,
                };

                let left_text = &ctx.source[bin.left.span().start as usize..bin.left.span().end as usize];
                let right_text = &ctx.source[bin.right.span().start as usize..bin.right.span().end as usize];

                // Avoid false positives on single-char tokens for `-` and `/`.
                if (op_str == "-" || op_str == "/") && left_text.len() <= 1 {
                    return;
                }

                if left_text == right_text {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, bin.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Identical expression `{left_text}` on both sides of `{op_str}`."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            AstKind::LogicalExpression(logical) => {
                let op_str = match logical.operator {
                    oxc_ast::ast::LogicalOperator::And => "&&",
                    oxc_ast::ast::LogicalOperator::Or => "||",
                    _ => return,
                };

                let left_text = &ctx.source
                    [logical.left.span().start as usize..logical.left.span().end as usize];
                let right_text = &ctx.source
                    [logical.right.span().start as usize..logical.right.span().end as usize];

                if left_text == right_text {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, logical.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Identical expression `{left_text}` on both sides of `{op_str}`."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            _ => {}
        }
    }
}
