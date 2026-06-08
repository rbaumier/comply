//! elysia-ws-message-no-schema oxc backend.

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

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "ws" {
            return;
        }

        let text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let has_body = text.contains("body:") || text.contains("body :");
        let has_message = text.contains("message:") || text.contains("message :");
        if !has_body || has_message {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "WebSocket route declares `body:` but no `message:` \u{2014} incoming frames are not validated.".into(),
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
    fn flags_ws_with_body_no_message() {
        let src = "import { Elysia, t } from 'elysia';\napp.ws('/chat', { body: t.Object({}), open: () => {} });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_ws_with_message_schema() {
        let src = "import { Elysia, t } from 'elysia';\napp.ws('/chat', { body: t.Object({}), message: t.String() });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_ws_without_body() {
        let src = "import { Elysia } from 'elysia';\napp.ws('/chat', { open: () => {} });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.ws('/chat', { body: t.Object({}) });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
