//! OXC backend for elysia-route-missing-auth.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const SENSITIVE: &[&str] = &["/admin", "/profile", "/me", "/settings", "/user"];
const HTTP_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "all"];

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

        // Callee must be a member expression like `app.get`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !HTTP_METHODS.contains(&method) {
            return;
        }

        // Get full text of arguments to match the TS backend's text-based approach.
        let args_text =
            &ctx.source[call.span.start as usize..call.span.end as usize];

        let has_sensitive_path = SENSITIVE.iter().any(|p| {
            args_text.contains(&format!("'{}", p))
                || args_text.contains(&format!("\"{}", p))
                || args_text.contains(&format!("`{}", p))
        });
        if !has_sensitive_path {
            return;
        }

        if args_text.contains("beforeHandle")
            || args_text.contains("auth")
            || args_text.contains("guard")
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Sensitive route appears to have no auth guard — add `beforeHandle` or wrap it in `.guard({ auth: ... })`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
