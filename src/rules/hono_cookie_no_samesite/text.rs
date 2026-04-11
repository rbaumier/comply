use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn has_hono_cookie_import(source: &str) -> bool {
    source.contains("hono/cookie")
}

/// Check if `sameSite` is set to a safe value (`'Lax'` or `'Strict'`) within a window of lines.
fn has_safe_samesite(lines: &[&str], idx: usize) -> bool {
    let end = (idx + 4).min(lines.len());
    for line in &lines[idx..end] {
        let norm: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        if norm.contains("sameSite:") || norm.contains("sameSite :") {
            // Present — check it's not 'None'.
            let lower = norm.to_lowercase();
            if lower.contains("'none'") || lower.contains("\"none\"") {
                return false;
            }
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
            if line.contains("setCookie(") && !has_safe_samesite(&lines, idx) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "hono-cookie-no-samesite".into(),
                    message: "`setCookie()` without `sameSite` or with `sameSite: 'None'` — vulnerable to CSRF.".into(),
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
    fn flags_set_cookie_without_samesite() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, { httpOnly: true });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_samesite_none() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, { sameSite: 'None' });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_samesite_lax() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, { sameSite: 'Lax' });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_samesite_strict() {
        let src = "import { setCookie } from 'hono/cookie';\nsetCookie(c, 'token', val, {\n  sameSite: 'Strict'\n});";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_cookie_files() {
        let src = "setCookie(c, 'token', val, {});";
        assert!(run(src).is_empty());
    }
}
