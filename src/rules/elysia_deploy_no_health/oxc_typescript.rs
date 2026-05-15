use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const HEALTH_PATHS: &[&str] = &[
    "/health",
    "/healthz",
    "/readyz",
    "/livez",
    "/_health",
    "/health/live",
    "/health/ready",
];

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
        if !ctx.project.has_framework("elysia") {
            return;
        }
        if !ctx.source.contains(".listen(") {
            return;
        }
        if HEALTH_PATHS.iter().any(|p| ctx.source.contains(p)) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        // Check callee ends with `.listen`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name != "listen" {
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
            rule_id: "elysia-deploy-no-health".into(),
            message: "Elysia server exposes `.listen()` without a `/health` endpoint — orchestrators lack a liveness probe.".into(),
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
    fn flags_app_listen_without_health() {
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
