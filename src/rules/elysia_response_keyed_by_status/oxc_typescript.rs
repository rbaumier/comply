//! elysia-response-keyed-by-status oxc backend — `response: t.X(...)` (no status
//! keying) hides error variants from the typed client.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

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

        // Callee must be a member expression with a route method name.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop_text = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&prop_text) {
            return;
        }

        let call_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let norm: String = call_text.chars().filter(|c| !c.is_whitespace()).collect();

        // `response:t.` indicates a bare TypeBox schema (no status keying).
        if !norm.contains("response:t.") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use a status-keyed response: `response: { 200: t.Object({...}), 4xx: ... }` so error shapes are typed.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
