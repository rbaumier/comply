//! elysia-cookie-no-samesite backend — flag cookie configs without explicit sameSite.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }

        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            let is_cookie_config = line.contains("t.Cookie(") || line.contains("cookie.set({");
            if !is_cookie_config {
                continue;
            }
            let block = collect_cookie_block(&lines, idx);
            let norm: String = block.chars().filter(|c| !c.is_whitespace()).collect();
            if norm.contains("sameSite:") {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: 1,
                rule_id: "elysia-cookie-no-samesite".into(),
                message: "Cookie config has no explicit `sameSite` — set `'lax'` or `'strict'`."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

/// Scan lines starting at `start` until the parenthesis opened by
/// `t.Cookie(` / `cookie.set(` on the trigger line is balanced or we hit a
/// 20-line cap. Returns the joined block text. Tracking parens (not braces)
/// keeps the whole call — including nested option objects on later lines.
fn collect_cookie_block(lines: &[&str], start: usize) -> String {
    const MAX_LINES: usize = 20;
    let end = (start + MAX_LINES).min(lines.len());
    let mut depth: i32 = 0;
    let mut seen_open = false;
    let mut last = start;
    for i in start..end {
        for ch in lines[i].chars() {
            match ch {
                '(' => {
                    depth += 1;
                    seen_open = true;
                }
                ')' => {
                    depth -= 1;
                }
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
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_missing_samesite() {
        let src = "import { Elysia, t } from 'elysia';\nt.Cookie({ token: t.String() }, { httpOnly: true });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_with_samesite() {
        let src = "import { Elysia, t } from 'elysia';\nt.Cookie({ token: t.String() }, { httpOnly: true, sameSite: 'lax' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "t.Cookie({ token: t.String() });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }

    #[test]
    fn allows_multiline_cookie_with_samesite_beyond_six_lines() {
        let src = "import { Elysia, t } from 'elysia';\nt.Cookie(\n  { token: t.String() },\n  {\n    httpOnly: true,\n    secure: true,\n    path: '/',\n    domain: 'example.com',\n    maxAge: 3600,\n    sameSite: 'lax',\n  },\n);";
        assert!(run_on(src).is_empty());
    }
}
