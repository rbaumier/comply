//! post-message-origin OxcCheck backend.

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Only `Window.postMessage` takes a `targetOrigin`; `BroadcastChannel`,
        // `Worker`, and `MessagePort` expose a one-argument `postMessage` with no
        // such parameter, so flag only window-like member receivers. A bare
        // `postMessage(data)` is the worker global's `postMessage` (no
        // `targetOrigin`) and is intentionally not flagged.
        let is_post_message = match &call.callee {
            Expression::StaticMemberExpression(member) => {
                member.property.name.as_str() == "postMessage"
                    && is_window_like_post_message_target(&member.object)
            }
            _ => false,
        };

        if !is_post_message {
            return;
        }

        // postMessage(message, targetOrigin, [transfer])
        // Check second argument (targetOrigin)
        let is_unsafe = if call.arguments.len() < 2 {
            true // Missing targetOrigin
        } else {
            let arg = &call.arguments[1];
            match arg {
                oxc_ast::ast::Argument::StringLiteral(lit) => lit.value.as_str() == "*",
                oxc_ast::ast::Argument::TemplateLiteral(tpl) => {
                    tpl.expressions.is_empty()
                        && tpl.quasis.len() == 1
                        && tpl.quasis[0].value.raw.as_str() == "*"
                }
                _ => false,
            }
        };

        if !is_unsafe {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`postMessage()` with `'*'` or missing target origin — specify explicit origin."
                .into(),
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
    fn flags_window_missing_origin() {
        assert_eq!(run("window.postMessage(data)").len(), 1);
    }

    #[test]
    fn flags_window_wildcard_origin() {
        assert_eq!(run("window.postMessage(data, '*')").len(), 1);
    }

    #[test]
    fn flags_iframe_content_window() {
        assert_eq!(run("iframe.contentWindow.postMessage(data)").len(), 1);
    }

    #[test]
    fn allows_window_explicit_origin() {
        assert!(run("window.postMessage(data, 'https://example.com')").is_empty());
    }

    #[test]
    fn ignores_broadcast_channel() {
        // Regression for #1838 — BroadcastChannel.postMessage takes no targetOrigin.
        assert!(run("this.channel.postMessage({ type: 'db:update' })").is_empty());
        assert!(run("new BroadcastChannel('x').postMessage(msg)").is_empty());
    }

    #[test]
    fn ignores_worker_and_message_port() {
        // Worker / MessagePort postMessage have no targetOrigin parameter.
        assert!(run("worker.postMessage(msg)").is_empty());
        assert!(run("port.postMessage(msg)").is_empty());
    }

    #[test]
    fn ignores_worker_global_scope() {
        // Regression for #1655 — inside a worker, `self`/`globalThis`/bare
        // `postMessage` is DedicatedWorkerGlobalScope.postMessage(message,
        // transfer), which has no targetOrigin parameter.
        assert!(run("self.postMessage({ msg: 'load worker' })").is_empty());
        assert!(run("globalThis.postMessage({ msg: 'load worker' })").is_empty());
        assert!(run("postMessage({ msg: 'load worker' })").is_empty());
        assert!(run("const port = event.ports[0]; port.postMessage({ type: 'error' })").is_empty());
    }

    #[test]
    fn still_flags_genuine_cross_origin() {
        // Negative-space guard for #1655 — genuine cross-origin window/frame
        // receivers missing a targetOrigin must still fire.
        assert_eq!(run("window.postMessage(data)").len(), 1);
        assert_eq!(run("iframe.contentWindow.postMessage(data)").len(), 1);
        assert_eq!(run("parent.postMessage(data)").len(), 1);
    }
}
