//! security-require-rate-limit-auth OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_auth_path(path: &str) -> bool {
    let unquoted = path.trim_matches(|c: char| c == '"' || c == '\'' || c == '`');
    let lower = unquoted.to_ascii_lowercase();
    lower.contains("/login")
        || lower.contains("/signin")
        || lower.contains("/sign-in")
        || lower.contains("/signup")
        || lower.contains("/sign-up")
        || lower.contains("/register")
        || lower.contains("/reset")
        || lower.contains("/forgot-password")
}

fn looks_like_rate_limit(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("ratelimit")
        || lower.contains("rate_limit")
        || lower.contains("rate-limit")
        || lower.contains("ratelimiter")
        || lower.contains("throttle")
        || lower.contains("slow-down")
        || lower.contains("slowdown")
}

fn has_global_rate_limit(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    for (i, _) in lower.match_indices(".use(") {
        let end = (i + 125).min(lower.len());
        let window = &lower[i..end];
        if looks_like_rate_limit(window) {
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Check callee is a member expression like app.post, router.get, etc.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !matches!(method, "post" | "get" | "put" | "patch" | "all") {
            return;
        }

        // First argument must be a string literal with an auth path.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let path_text = match first_arg {
            Argument::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if !is_auth_path(path_text) {
            return;
        }

        // Check remaining args for rate-limit middleware.
        for arg in call.arguments.iter().skip(1) {
            let text = &ctx.source[arg.span().start as usize..arg.span().end as usize];
            if looks_like_rate_limit(text) {
                return;
            }
        }

        // Check for global rate-limit middleware.
        if has_global_rate_limit(ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Auth route \"{path_text}\" has no rate-limit middleware — attackers can brute-force credentials."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
