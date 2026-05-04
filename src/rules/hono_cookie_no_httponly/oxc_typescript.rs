//! hono-cookie-no-httponly OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

fn has_hono_cookie_import(source: &str) -> bool {
    source.contains("hono/cookie")
}

fn has_http_only(lines: &[&str], idx: usize) -> bool {
    let end = (idx + 4).min(lines.len());
    for line in &lines[idx..end] {
        let norm: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        if norm.contains("httpOnly:true") {
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

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !has_hono_cookie_import(ctx.source) {
            return Vec::new();
        }

        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            if line.contains("setCookie(") && !has_http_only(&lines, idx) {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`setCookie()` without `httpOnly: true` — cookie is \
                              accessible to JavaScript (XSS vector)."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}
