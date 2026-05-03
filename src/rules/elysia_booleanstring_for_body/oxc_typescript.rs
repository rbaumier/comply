use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const STOP_KEYS: &[&str] = &[
    "params:",
    "query:",
    "headers:",
    "response:",
    "cookie:",
    "detail:",
    "tags:",
];

const ROUTE_METHODS: &[&str] = &["post", "put", "patch", "delete"];

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

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let prop_text = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&prop_text) {
            return;
        }

        let args_text =
            &ctx.source[call.span.start as usize..call.span.end as usize];
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

        let Some(body_idx) = norm.find("body:t.") else {
            return;
        };
        let after_body = &norm[body_idx..];
        let cut = STOP_KEYS
            .iter()
            .filter_map(|k| after_body[1..].find(k).map(|i| i + 1))
            .min()
            .unwrap_or(after_body.len());
        let body_section = &after_body[..cut];

        if !body_section.contains("t.Boolean(") {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`t.Boolean()` in a `body:` schema rejects `\"true\"`/`\"false\"` — use `t.BooleanString()` for form-encoded payloads.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
