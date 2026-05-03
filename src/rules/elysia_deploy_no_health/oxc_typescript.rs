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
