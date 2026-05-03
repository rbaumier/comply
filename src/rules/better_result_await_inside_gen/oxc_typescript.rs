//! better-result-await-inside-gen oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AwaitExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Result.gen"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AwaitExpression(await_expr) = node.kind() else {
            return;
        };
        // Walk ancestors to see if we're inside a Result.gen call.
        // Stop at the first Result.gen we find (don't cross into nested ones).
        if !is_inside_result_gen(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Inside Result.gen, use `yield* Result.await(...)` instead of `await`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Walk ancestors to check if this node is inside a `Result.gen(...)` call.
/// Returns false if we hit a nested `Result.gen` boundary first (the inner
/// gen has its own scope).
fn is_inside_result_gen<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::CallExpression(call) = ancestor.kind() {
            if let Expression::StaticMemberExpression(member) = &call.callee {
                if member.property.name.as_str() == "gen" {
                    if let Expression::Identifier(obj) = &member.object {
                        if obj.name.as_str() == "Result" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}
