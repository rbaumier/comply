//! no-redundant-await OXC backend — flag `return await x;` that is not inside
//! a `try` block.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Walk up the semantic parent chain, stopping at the nearest function
/// boundary. Return `true` when the return sits where `return await` is
/// load-bearing: inside a `try` BODY block, or inside the `catch` handler of a
/// `try` that also has a `finally` (there the `await` decides whether the
/// finalizer runs before or after the returned promise settles).
fn is_inside_try_body(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
    return_start: u32,
    return_end: u32,
) -> bool {
    let mut current_id = node_id;
    loop {
        let parent_id = semantic.nodes().parent_id(current_id);
        if parent_id == current_id {
            break;
        }
        let n = semantic.nodes().get_node(parent_id);
        match n.kind() {
            // Function boundary — stop.
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            AstKind::TryStatement(try_stmt) => {
                // Check we are in the body block, not catch/finally.
                let body_start = try_stmt.block.span.start;
                let body_end = try_stmt.block.span.end;
                if return_start >= body_start && return_end <= body_end {
                    return true;
                }
                // A `return await` inside the catch handler of a try that has a
                // `finally` is load-bearing: the await decides whether the
                // finalizer runs before or after the returned promise settles,
                // so removing it changes behavior.
                if try_stmt.finalizer.is_some()
                    && let Some(handler) = &try_stmt.handler
                    && return_start >= handler.span.start
                    && return_end <= handler.span.end
                {
                    return true;
                }
            }
            _ => {}
        }
        current_id = parent_id;
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ReturnStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ReturnStatement(ret) = node.kind() else {
            return;
        };
        let Some(arg) = &ret.argument else {
            return;
        };

        // Unwrap parenthesized expressions.
        let mut expr = arg;
        while let Expression::ParenthesizedExpression(paren) = expr {
            expr = &paren.expression;
        }

        let Expression::AwaitExpression(await_expr) = expr else {
            return;
        };

        if is_inside_try_body(node.id(), semantic, ret.span.start, ret.span.end) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Redundant `return await` outside a try block — drop the \
                      `await` and return the promise directly."
                .into(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_return_await_in_catch_with_finally() {
        // #6425: with a `finally` present, the `await` decides whether the
        // finalizer runs before or after the returned promise settles.
        let d = run_on(
            "async function f() {\n\
               try { await f(); }\n\
               catch (e) { return await onError(e); }\n\
               finally { cleanup(); }\n\
             }",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn flags_return_await_in_catch_without_finally() {
        // No `finally` → nothing to time, so the `await` is redundant.
        let d = run_on(
            "async function f() {\n\
               try { await f(); }\n\
               catch (e) { return await onError(e); }\n\
             }",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_plain_return_await_outside_try() {
        let d = run_on("async function g() { return await h(); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_return_await_in_try_body() {
        let d = run_on(
            "async function f() {\n\
               try { return await h(); }\n\
               catch (e) { handle(e); }\n\
             }",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn flags_return_await_in_finally_block() {
        // The exemption keys off the catch handler span, not the finalizer
        // span, so a return inside the `finally` itself stays flagged.
        let d = run_on(
            "async function f() {\n\
               try { await f(); }\n\
               finally { return await cleanup(); }\n\
             }",
        );
        assert_eq!(d.len(), 1);
    }
}
