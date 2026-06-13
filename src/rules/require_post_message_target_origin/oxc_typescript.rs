//! OxcCheck backend — flag `.postMessage(data)` missing `targetOrigin`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_window_like_post_message_target};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["postMessage"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        // Callee must be `*.postMessage`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "postMessage" {
            return;
        }
        // Only `Window.postMessage` takes a `targetOrigin`; `BroadcastChannel`,
        // `Worker`, and `MessagePort` expose a one-argument `postMessage` with no
        // such parameter, so flag only window-like receivers.
        if !is_window_like_post_message_target(&member.object) {
            return;
        }
        // Must have exactly 1 argument (data, no targetOrigin).
        // 0 means no data either, 2+ means origin is provided.
        if call.arguments.len() != 1 {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`postMessage()` called without `targetOrigin` \u{2014} provide an explicit origin.".into(),
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod oxc_tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_window_single_arg() {
        assert_eq!(run("window.postMessage(data);").len(), 1);
    }

    #[test]
    fn flags_iframe_content_window_single_arg() {
        assert_eq!(run("iframe.contentWindow.postMessage(data);").len(), 1);
    }

    #[test]
    fn allows_window_with_origin() {
        assert!(run(r#"window.postMessage(data, "https://example.com");"#).is_empty());
    }

    #[test]
    fn ignores_broadcast_channel() {
        // Regression for #1838 — BroadcastChannel.postMessage takes no targetOrigin.
        assert!(run("this.channel.postMessage({ type: 'db:update' });").is_empty());
        assert!(run("new BroadcastChannel('x').postMessage(msg);").is_empty());
    }

    #[test]
    fn ignores_worker_and_message_port() {
        // Worker / MessagePort postMessage have no targetOrigin parameter.
        assert!(run("worker.postMessage(msg);").is_empty());
        assert!(run("port.postMessage(msg);").is_empty());
    }
}
