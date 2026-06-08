//! hono-cookie-no-httponly OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

fn has_hono_cookie_import(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "hono/cookie")
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

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_set_cookie_without_httponly() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, { secure: true });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_set_cookie_with_httponly() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, { httpOnly: true, secure: true });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_hono_cookie_files() {
        let src = "setCookie(c, 'token', val, {});";
        assert!(run_on(src).is_empty());
    }
}
