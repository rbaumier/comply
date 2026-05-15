//! elysia-listen-callback-info oxc backend — flag .listen() with no callback.

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
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }

        // Callee must be `*.listen`.
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

        // Must have exactly one argument.
        if call.arguments.len() != 1 {
            return;
        }

        // If the single arg is already a callback, don't flag.
        let Some(expr) = call.arguments[0].as_expression() else { return };
        if matches!(
            expr,
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
        ) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.listen(...)` has no callback — pass one and log the server info so deploys show where the server is bound.".into(),
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
    fn flags_app_listen_without_callback() {
        let src = "app.listen(3000);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_app_listen_with_callback() {
        let src = "app.listen(3000, () => console.log('ok'));";
        assert!(run(src).is_empty());
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
