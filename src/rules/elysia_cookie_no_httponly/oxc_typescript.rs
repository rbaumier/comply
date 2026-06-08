//! elysia-cookie-no-httponly OXC backend — flag cookie configs without httpOnly: true.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["t.Cookie(", "cookie.set("])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }

        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            if !line.contains("t.Cookie(") && !line.contains("cookie.set({") {
                continue;
            }
            let block = collect_cookie_block(&lines, idx);
            let norm: String = block.chars().filter(|c| !c.is_whitespace()).collect();
            if norm.contains("httpOnly:true") {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Cookie config is missing `httpOnly: true` \u{2014} readable from JavaScript (XSS).".into(),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
}

fn collect_cookie_block(lines: &[&str], start: usize) -> String {
    const MAX_LINES: usize = 20;
    let end = (start + MAX_LINES).min(lines.len());
    let mut depth: i32 = 0;
    let mut seen_open = false;
    let mut last = start;
    for i in start..end {
        for ch in lines[i].chars() {
            match ch {
                '(' => { depth += 1; seen_open = true; }
                ')' => { depth -= 1; }
                _ => {}
            }
        }
        last = i;
        if seen_open && depth <= 0 {
            break;
        }
    }
    lines[start..=last].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_missing_httponly() {
        let src = "import { Elysia, t } from 'elysia';\nt.Cookie({ token: t.String() }, { secure: true });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_with_httponly() {
        let src = "import { Elysia, t } from 'elysia';\nt.Cookie({ token: t.String() }, { httpOnly: true, secure: true });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "t.Cookie({ token: t.String() });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }


    #[test]
    fn allows_multiline_cookie_with_httponly_beyond_six_lines() {
        let src = "import { Elysia, t } from 'elysia';\nt.Cookie(\n  { token: t.String() },\n  {\n    secure: true,\n    sameSite: 'lax',\n    path: '/',\n    domain: 'example.com',\n    maxAge: 3600,\n    httpOnly: true,\n  },\n);";
        assert!(run_on(src).is_empty());
    }
}
