//! hono-cookie-no-samesite OXC backend — flag `setCookie()` without safe `sameSite`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn has_hono_cookie_import(source: &str) -> bool {
    source.contains("hono/cookie")
}

fn has_safe_samesite(lines: &[&str], idx: usize) -> bool {
    let end = (idx + 4).min(lines.len());
    for line in &lines[idx..end] {
        let norm: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        if norm.contains("sameSite:") || norm.contains("sameSite :") {
            let lower = norm.to_lowercase();
            if lower.contains("'none'") || lower.contains("\"none\"") {
                return false;
            }
            return true;
        }
    }
    false
}

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
        if !has_hono_cookie_import(ctx.source) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        // Check callee is `setCookie`
        let callee_span = call.callee.span();
        let callee_text = &ctx.source[callee_span.start as usize..callee_span.end as usize];
        if callee_text != "setCookie" {
            return;
        }

        // Use the same line-based heuristic as the TreeSitter version
        let (line, _column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        let lines: Vec<&str> = ctx.source.lines().collect();
        let idx = line.saturating_sub(1); // line is 1-based
        if has_safe_samesite(&lines, idx) {
            return;
        }

        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: 1,
            rule_id: super::META.id.into(),
            message: "`setCookie()` without `sameSite` or with \
                      `sameSite: 'None'` — vulnerable to CSRF."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
