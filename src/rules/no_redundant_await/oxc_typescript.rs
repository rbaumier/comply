//! no-redundant-await OXC backend — flag `return await x;` that is not inside
//! a `try` block.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Walk up the semantic parent chain. Return `true` if a `TryStatement` body
/// is encountered before a function boundary.
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
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_return_await() {
        let d = run_on("async function f() { return await g(); }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-redundant-await");
    }


    #[test]
    fn flags_return_await_in_arrow() {
        let d = run_on("const f = async () => { return await g(); };");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_return_await_inside_try() {
        assert!(
            run_on("async function f() { try { return await g(); } catch (e) { throw e; } }")
                .is_empty()
        );
    }


    #[test]
    fn flags_return_await_inside_catch() {
        // In catch, the enclosing try no longer helps — catch handles its own errors.
        // But since catch is not inside the try's body field, it's still redundant.
        let d = run_on("async function f() { try { x(); } catch (e) { return await g(); } }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_return_without_await() {
        assert!(run_on("async function f() { return g(); }").is_empty());
    }


    #[test]
    fn allows_await_without_return() {
        assert!(run_on("async function f() { await g(); }").is_empty());
    }
}
