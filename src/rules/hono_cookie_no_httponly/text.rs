use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !has_hono_cookie_import(ctx.source) {
            return Vec::new();
        }

        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            if line.contains("setCookie(") && !has_http_only(&lines, idx) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "hono-cookie-no-httponly".into(),
                    message: "`setCookie()` without `httpOnly: true` — cookie is accessible to JavaScript (XSS vector).".into(),
                    severity: Severity::Error,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_set_cookie_without_httponly() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, { secure: true });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_set_cookie_with_httponly() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, { httpOnly: true, secure: true });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_httponly_on_next_line() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, {\n  httpOnly: true,\n  secure: true\n});";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_cookie_files() {
        let src = "setCookie(c, 'token', val, {});";
        assert!(run(src).is_empty());
    }
}
