//! security-require-oauth-state OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn validates_state(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.contains("state===")
        || compact.contains("state!==")
        || compact.contains("state==")
        || compact.contains("state!=")
        || compact.contains("===state")
        || compact.contains("!==state")
        || compact.contains("==state")
        || compact.contains("!=state")
    {
        return true;
    }
    if compact.contains(".state")
        || compact.contains("[\"state\"]")
        || compact.contains("['state']")
    {
        return true;
    }
    if compact.contains(".get(\"state\")") || compact.contains(".get('state')") {
        return true;
    }
    let lower = compact.to_ascii_lowercase();
    if lower.contains("verifystate")
        || lower.contains("validatestate")
        || lower.contains("checkstate")
        || lower.contains("assertstate")
    {
        return true;
    }
    for marker in ["verify(", "validate(", "check(", "assert("] {
        if let Some(idx) = lower.find(marker) {
            let tail = &lower[idx + marker.len()..];
            if tail.starts_with("state") {
                return true;
            }
        }
    }
    false
}

fn is_oauth_callback_path(path: &str) -> bool {
    let unquoted = path.trim_matches(|c: char| c == '"' || c == '\'' || c == '`');
    let lower = unquoted.to_ascii_lowercase();
    lower.contains("/callback")
        || lower.contains("/oauth/callback")
        || lower.contains("/auth/callback")
        || lower.ends_with("/cb")
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

        // Callee must be `.get`, `.post`, or `.all`
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !matches!(method, "get" | "post" | "all") {
            return;
        }

        // First argument must be a string with an OAuth callback path
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let path_text = match first_arg {
            oxc_ast::ast::Argument::StringLiteral(s) => {
                let raw = &ctx.source[s.span.start as usize..s.span.end as usize];
                raw.to_string()
            }
            _ => return,
        };
        if !is_oauth_callback_path(&path_text) {
            return;
        }

        // Check handler arguments (skip first arg = path) for state validation
        let mut reads_state = false;
        for arg in call.arguments.iter().skip(1) {
            let span = arg.span();
            let text = &ctx.source[span.start as usize..span.end as usize];
            let stripped = strip_comments(text);
            if validates_state(&stripped) {
                reads_state = true;
                break;
            }
        }
        if reads_state {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "OAuth callback handler {path_text} never reads `state` — CSRF validation is missing."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
