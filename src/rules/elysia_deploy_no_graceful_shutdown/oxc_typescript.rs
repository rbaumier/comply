//! OXC backend for elysia-deploy-no-graceful-shutdown.

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
        Some(&[".listen"])
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
        if !ctx.source_contains(".listen(") {
            return;
        }
        // If the file already wires shutdown signals OR calls `.stop()`, accept it.
        if ctx.source_contains("SIGTERM") || ctx.source_contains("SIGINT") || ctx.source_contains(".stop()") {
            return;
        }

        // callee must end with `.listen`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "listen" {
            return;
        }

        // Skip non-Elysia receivers (e.g. MSW's mswServer.listen()).
        let Some(root) =
            crate::rules::elysia_helpers::root_identifier_name(&member.object)
        else {
            return;
        };
        if !crate::rules::elysia_helpers::looks_like_elysia_identifier(root) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Elysia `.listen()` without SIGTERM/SIGINT handler — in-flight requests will be dropped on shutdown.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(src, &Check, "elysia")
    }

    #[test]
    fn flags_app_listen_without_signal_handler() {
        let src = "app.listen(3000);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_msw_server_listen() {
        // Regression for rbaumier/comply#21.
        let src = r#"
            const mswServer = setupServer();
            mswServer.listen({ onUnhandledRequest: "error" });
        "#;
        assert!(run(src).is_empty());
    }
}
