//! prefer-optional-catch-binding OXC backend — flag unused catch binding params.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BindingPattern;
use std::sync::Arc;

pub struct Check;

/// Check if `name` appears as a word-boundary identifier in `body_text`.
fn is_name_used_in(body_text: &str, name: &str) -> bool {
    let name_bytes = name.as_bytes();
    let body_bytes = body_text.as_bytes();
    let mut pos = 0;
    while pos + name_bytes.len() <= body_bytes.len() {
        if let Some(found) = body_text[pos..].find(name) {
            let abs = pos + found;
            let before_ok = abs == 0
                || (!body_bytes[abs - 1].is_ascii_alphanumeric() && body_bytes[abs - 1] != b'_');
            let after = abs + name_bytes.len();
            let after_ok = after >= body_bytes.len()
                || (!body_bytes[after].is_ascii_alphanumeric() && body_bytes[after] != b'_');
            if before_ok && after_ok {
                return true;
            }
            pos = abs + 1;
        } else {
            break;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else {
            return;
        };
        let Some(handler) = &try_stmt.handler else {
            return;
        };
        let Some(param) = &handler.param else {
            return; // Already omitted — `catch { ... }`
        };

        let param_name = match &param.pattern {
            BindingPattern::BindingIdentifier(id) => id.name.as_str(),
            _ => return, // Destructuring patterns — skip.
        };

        if param_name.is_empty() {
            return;
        }

        // Use source text of the catch body to check for usage.
        let body_span = handler.body.span;
        let body_text = &ctx.source[body_span.start as usize..body_span.end as usize];
        // Skip the opening `{` to avoid matching the param in the catch clause itself.
        let inner = body_text.strip_prefix('{').unwrap_or(body_text);

        if !is_name_used_in(inner, param_name) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, param.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Unused catch binding `{param_name}`. Remove it: use `catch {{ … }}` instead of `catch ({param_name}) {{ … }}`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
