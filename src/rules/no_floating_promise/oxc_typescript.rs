use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const ASYNC_LOOKING_METHODS: &[&str] = &[
    "send", "save", "load", "fetch", "query", "emit", "publish", "write", "insert", "update",
    "delete", "close", "connect", "dispatch", "sync", "flush", "commit", "rollback", "run", "exec",
    "execute", "process", "handle",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExpressionStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ExpressionStatement(stmt) = node.kind() else {
            return;
        };
        let Expression::CallExpression(call) = &stmt.expression else {
            return;
        };

        // Check if already handled by .then/.catch/.finally
        if has_promise_handler(call) {
            return;
        }

        let is_flag = is_promise_combinator(call) || is_async_looking_member_call(call);
        if !is_flag {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Promise-returning call is used as a statement \u{2014} rejections will \
                      become UnhandledPromiseRejection. Add `await`, chain `.catch`, \
                      or prefix with `void` if you really want to ignore it."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

use oxc_ast::ast::*;

/// Does the call end with `.then(...)` / `.catch(...)` / `.finally(...)`?
fn has_promise_handler(call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    matches!(
        member.property.name.as_str(),
        "then" | "catch" | "finally"
    )
}

/// Is the callee `Promise.<combinator>`?
fn is_promise_combinator(call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    if obj.name.as_str() != "Promise" {
        return false;
    }
    matches!(
        member.property.name.as_str(),
        "resolve" | "reject" | "all" | "allSettled" | "race" | "any"
    )
}

/// Is the callee a member whose method name is in the async-looking list?
fn is_async_looking_member_call(call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let method = member.property.name.as_str();
    ASYNC_LOOKING_METHODS.contains(&method)
}
