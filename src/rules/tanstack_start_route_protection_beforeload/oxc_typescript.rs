//! OXC backend for tanstack-start-route-protection-beforeload.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const AUTH_PATH_MARKERS: &[&str] = &["login", "signin", "sign-in", "auth", "authenticate"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["beforeLoad"])
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

        // Callee must be `useEffect`
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != "useEffect" {
            return;
        }

        // First argument must be a function (arrow or function expression)
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let cb_expr = match first_arg {
            oxc_ast::ast::Argument::ArrowFunctionExpression(arrow) => {
                if !contains_auth_redirect_in_arrow(arrow, ctx.source) {
                    return;
                }
                call.span
            }
            oxc_ast::ast::Argument::FunctionExpression(func) => {
                if !contains_auth_redirect_in_function(func, ctx.source) {
                    return;
                }
                call.span
            }
            _ => return,
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, cb_expr.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Don't redirect to an auth route from `useEffect`. Move the guard to \
                     `beforeLoad` and `throw redirect({ to: '/login' })` on the route."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn contains_auth_redirect_in_arrow(arrow: &oxc_ast::ast::ArrowFunctionExpression, source: &str) -> bool {
    let body_text = &source[arrow.span.start as usize..arrow.span.end as usize];
    contains_auth_redirect_text(body_text)
}

fn contains_auth_redirect_in_function(func: &oxc_ast::ast::Function, source: &str) -> bool {
    let body_text = &source[func.span.start as usize..func.span.end as usize];
    contains_auth_redirect_text(body_text)
}

/// Text-based check for auth redirect patterns inside a callback body.
/// This mirrors the tree-sitter version which walks all descendants looking for:
/// 1. Call-form: navigate('/login'), router.push('/login'), redirect('/login')
/// 2. Assignment: window.location = '/login', window.location.href = '/login'
/// 3. Method: window.location.assign('/login'), window.location.replace('/login')
fn contains_auth_redirect_text(body: &str) -> bool {
    // Quick bail: body must contain at least one auth-looking path marker
    if !AUTH_PATH_MARKERS.iter().any(|m| body.to_ascii_lowercase().contains(m)) {
        return false;
    }

    // Check for redirect callees with auth path arguments
    let has_redirect_call = (body.contains("navigate(") || body.contains(".push(")
        || body.contains(".replace(") || body.contains("redirect("))
        && AUTH_PATH_MARKERS.iter().any(|m| body.to_ascii_lowercase().contains(m));

    // Check for window.location assignments
    let has_location_assign = body.contains("window.location")
        && AUTH_PATH_MARKERS.iter().any(|m| body.to_ascii_lowercase().contains(m));

    has_redirect_call || has_location_assign
}
