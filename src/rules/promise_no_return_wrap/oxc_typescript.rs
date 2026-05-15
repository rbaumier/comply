//! promise-no-return-wrap oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// True if `expr` is `Promise.resolve(...)` / `Promise.reject(...)`.
fn is_promise_static_wrap(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    if obj.name.as_str() != "Promise" {
        return false;
    }
    matches!(member.property.name.as_str(), "resolve" | "reject")
}

/// Walk up ancestors until we find a CallExpression whose callee is
/// `<x>.then` / `<x>.catch` / `<x>.finally`.
fn inside_then_callback<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut current_id = node.id();
    for _ in 0..6 {
        let parent_id = semantic.nodes().parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = semantic.nodes().get_node(parent_id);
        if let AstKind::CallExpression(call) = parent.kind()
            && let Expression::StaticMemberExpression(member) = &call.callee
            && matches!(member.property.name.as_str(), "then" | "catch" | "finally")
        {
            return true;
        }
        if matches!(
            parent.kind(),
            AstKind::Function(_) | AstKind::Program(_)
        ) {
            return false;
        }
        current_id = parent_id;
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ReturnStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Promise.resolve(", "Promise.reject("])
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
        if !is_promise_static_wrap(arg) {
            return;
        }
        if !inside_then_callback(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, ret.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Wrapping a value in `Promise.resolve/reject` inside `.then()` \
                      is redundant — return the value directly (or `throw` to reject)."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
