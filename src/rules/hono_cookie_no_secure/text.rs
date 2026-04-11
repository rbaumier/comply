use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !has_hono_cookie_import(ctx.source) {
            return Vec::new();
        }

        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            if line.contains("setCookie(") && !has_secure(&lines, idx) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "hono-cookie-no-secure".into(),
                    message: "`setCookie()` without `secure: true` — cookie may be sent over unencrypted HTTP.".into(),
                    severity: Severity::Warning,
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
    fn flags_set_cookie_without_secure() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, { httpOnly: true });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_set_cookie_with_secure() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, { secure: true });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_secure_on_next_line() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, {\n  secure: true\n});";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_cookie_files() {
        let src = "setCookie(c, 'token', val, {});";
        assert!(run(src).is_empty());
    }
}
