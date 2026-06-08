//! OXC backend for elysia-ws-subscribe-before-publish.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".publish"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }
        if !ctx.source_contains(".ws(") {
            return;
        }

        // callee must end with `.publish`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "publish" {
            return;
        }

        if ctx.source_contains(".subscribe(") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`ws.publish()` is called but no client is `subscribe()`d to the topic — messages will be dropped.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_publish_without_subscribe() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().ws('/chat', { message(ws, msg) { ws.publish('room', msg); } });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_publish_with_subscribe() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().ws('/chat', { open(ws) { ws.subscribe('room'); }, message(ws, msg) { ws.publish('room', msg); } });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "ws.publish('room', msg);";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
