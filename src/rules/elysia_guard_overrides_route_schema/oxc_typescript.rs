//! elysia-guard-overrides-route-schema OXC backend — flag routes inside
//! `.guard({ body: ... })` that redeclare `body:`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options",
];

/// Check if an object expression contains a `body:` property.
fn object_has_body_key(args: &oxc_ast::ast::ObjectExpression) -> bool {
    for prop in &args.properties {
        if let ObjectPropertyKind::ObjectProperty(p) = prop
            && let PropertyKey::StaticIdentifier(key) = &p.key
                && key.name.as_str() == "body" {
                    return true;
                }
    }
    false
}

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
        // Check callee is `.guard(...)`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "guard" {
            return;
        }

        // First argument must be an object with `body:`.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let first_expr = match first_arg {
            Argument::ObjectExpression(obj) => {
                if !object_has_body_key(obj) {
                    return;
                }
                None
            }
            _ => {
                // Fallback: check source text of first arg.
                let span = first_arg.span();
                let text = &ctx.source[span.start as usize..span.end as usize];
                if !(text.contains("body:") || text.contains("body :")) {
                    return;
                }
                Some(())
            }
        };
        let _ = first_expr;

        // Now search the rest of the call source for a nested route call with body:.
        // Use source-text approach since the nested calls are deep descendants.
        let call_text = &ctx.source[call.span.start as usize..call.span.end as usize];

        // Find route methods in the call text.
        for method in ROUTE_METHODS {
            let pattern = format!(".{}(", method);
            if let Some(pos) = call_text.find(&pattern) {
                // Check if there's a `body:` after this route method.
                let after_route = &call_text[pos..];
                if after_route.contains("body:") || after_route.contains("body :") {
                    // The route is at offset `pos` from call start.
                    let byte_offset = call.span.start as usize + pos;
                    let (line, column) = byte_offset_to_line_col(ctx.source, byte_offset);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Route inside `.guard({ body: ... })` redeclares `body:` — the inner schema silently overrides the guard.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
            }
        }
    }
}
