//! hono-cookie-no-samesite backend — flag `setCookie()` without safe `sameSite`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
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

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        if !has_hono_cookie_import(ctx.source) {
            return Vec::new();
        }

        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            if line.contains("setCookie(") && !has_safe_samesite(&lines, idx) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "hono-cookie-no-samesite".into(),
                    message: "`setCookie()` without `sameSite` or with \
                              `sameSite: 'None'` — vulnerable to CSRF."
                        .into(),
                    severity: Severity::Warning,
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
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_set_cookie_without_samesite() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, { httpOnly: true });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_samesite_lax() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, { sameSite: 'Lax' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_samesite_none() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, { sameSite: 'None' });";
        assert_eq!(run_on(src).len(), 1);
    }
}
