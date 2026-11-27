//! no-empty-catch oxc backend. Flags an empty `catch (e) {}` block.
//! Flags a discarded `.catch(h)` or `.then(x, h)` too.
//! There, `h` returns `undefined`, `null` or `void 0`.
//! A comment inside allows it.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, UnaryOperator};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement, AstType::ExpressionStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let has_comment = |start: u32, end: u32| {
            let text = &ctx.source[start as usize..end as usize];
            text.contains("//") || text.contains("/*")
        };

        let (anchor, message) = match node.kind() {
            AstKind::TryStatement(try_stmt) => {
                let Some(handler) = &try_stmt.handler else {
                    return;
                };
                let body = &handler.body;
                if !body.body.is_empty() || has_comment(body.span.start, body.span.end) {
                    return;
                }
                (
                    handler.span.start,
                    "Empty catch block silently swallows the error — log it, rethrow, \
                     or add a comment explaining why.",
                )
            }
            // Value positions never reach this arm: only discarded calls.
            AstKind::ExpressionStatement(stmt) => {
                // Tests swallow rejections on purpose to probe other effects.
                if ctx.file.path_segments.in_test_dir {
                    return;
                }
                let discarded = match &stmt.expression {
                    Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::Void => {
                        &unary.argument
                    }
                    other => other,
                };
                let Expression::CallExpression(call) = discarded else {
                    return;
                };
                let handler_index = match call
                    .callee
                    .as_member_expression()
                    .and_then(|member| member.static_property_name())
                {
                    Some("catch") => 0,
                    Some("then") => 1,
                    _ => return,
                };
                let Some(Argument::ArrowFunctionExpression(arrow)) =
                    call.arguments.get(handler_index)
                else {
                    return;
                };
                let Some(value) = arrow.get_expression() else {
                    return;
                };
                let is_silent = value.is_null() || value.is_undefined() || value.is_void_0();
                if !is_silent || has_comment(arrow.span.start, arrow.span.end) {
                    return;
                }
                (
                    arrow.span.start,
                    "Promise rejection handler silently swallows the error — handle it, \
                     or add a comment explaining why.",
                )
            }
            _ => return,
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, anchor as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: message.into(),
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
