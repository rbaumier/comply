//! hono-cookie-no-secure oxc backend — flag `setCookie()` without `secure: true`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

fn has_hono_cookie_import(source: &str) -> bool {
    source.contains("hono/cookie")
}

fn has_secure(lines: &[&str], idx: usize) -> bool {
    let end = (idx + 4).min(lines.len());
    for line in &lines[idx..end] {
        let norm: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        if norm.contains("secure:true") {
            return true;
        }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["hono/cookie"])
    }

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
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !has_hono_cookie_import(ctx.source) {
            return;
        }

        let callee_text = &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if !callee_text.contains("setCookie") {
            return;
        }

        let (line, _column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        let lines: Vec<&str> = ctx.source.lines().collect();
        if has_secure(&lines, line.saturating_sub(1)) {
            return;
        }

        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: 1,
            rule_id: super::META.id.into(),
            message: "`setCookie()` without `secure: true` — cookie may be \
                      sent over unencrypted HTTP."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
