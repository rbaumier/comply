//! no-redundant-await OXC backend — flag `return await x;` that is not inside
//! a `try` block.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, VariableDeclarationKind};
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

/// Return `true` when the function enclosing the return statement contains a
/// `using` / `await using` declaration. There the trailing `await` in
/// `return await x` is load-bearing: per TC39 Explicit Resource Management the
/// declaration's resource is disposed when the enclosing block scope exits, so
/// dropping the `await` would dispose it before the returned promise settles —
/// an observable behavior change, the same disposal-ordering concern that
/// exempts `return await` inside a `try` block.
///
/// The search is bounded to the enclosing function's span. Any using-kind
/// declaration within that span suppresses the flag; this over-approximates
/// toward NOT flagging (a `using` in a nested closure also suppresses), the
/// false-positive-safe direction.
fn enclosing_function_contains_using(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();

    // Walk up to the nearest enclosing function to bound the search.
    let mut func_span = None;
    let mut current_id = node_id;
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            break;
        }
        match nodes.kind(parent_id) {
            AstKind::Function(f) => {
                func_span = Some(f.span);
                break;
            }
            AstKind::ArrowFunctionExpression(f) => {
                func_span = Some(f.span);
                break;
            }
            _ => {}
        }
        current_id = parent_id;
    }
    let Some(span) = func_span else {
        return false;
    };

    nodes.iter().any(|n| {
        let AstKind::VariableDeclaration(decl) = n.kind() else {
            return false;
        };
        matches!(
            decl.kind,
            VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing
        ) && decl.span.start >= span.start
            && decl.span.end <= span.end
    })
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

        if is_inside_try_body(node.id(), semantic, ret.span.start, ret.span.end)
            || enclosing_function_contains_using(node.id(), semantic)
        {
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
    fn allows_return_await_with_using_declaration() {
        // #7126: a `using` declaration disposes its resource when the function
        // scope exits, so the `await` orders disposal after the returned
        // promise settles — dropping it disposes prematurely.
        let d = run_on(
            "async function withTimeout(): Promise<string> {\n\
               using r = openResource();\n\
               return await computeResult();\n\
             }",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_return_await_with_await_using_declaration() {
        let d = run_on(
            "async function withTimeout(): Promise<string> {\n\
               await using r = openResource();\n\
               return await computeResult();\n\
             }",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn flags_return_await_when_using_is_in_a_different_function() {
        // The search is bounded to the enclosing function, so a `using` in a
        // sibling function must not suppress the flag here.
        let d = run_on(
            "function withResource() { using r = open(); doStuff(); }\n\
             async function redundant(): Promise<string> { return await compute(); }",
        );
        assert_eq!(d.len(), 1);
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
