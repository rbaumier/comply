//! elysia-jwt-cookie-no-httponly backend — flag cookie .set without httpOnly when jwt is used.

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
            if !line.contains(".set({") {
                continue;
            }
            if let Some(pos) = line.find(".set({") {
                let before = &line[..pos];
                if !before.contains("cookie") {
                    continue;
                }
            }
            let end = (idx + 6).min(lines.len());
            let block: String = lines[idx..end].join("\n");
            let norm: String = block.chars().filter(|c| !c.is_whitespace()).collect();
            if norm.contains("httpOnly:true") {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: 1,
                rule_id: "elysia-jwt-cookie-no-httponly".into(),
                message: "Cookie `.set({...})` without `httpOnly: true` — JWT is readable from JavaScript (XSS).".into(),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_cookie_set_without_httponly() {
        let src = "import { jwt } from '@elysiajs/jwt';\ncookie.auth.set({ value: token, secure: true });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_cookie_set_with_httponly() {
        let src = "import { jwt } from '@elysiajs/jwt';\ncookie.auth.set({ value: token, httpOnly: true, secure: true });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_cookie_set() {
        let src = "log.set({ requestId });\ndb.update(table).set({ role: 'admin' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "cookie.auth.set({ value: token });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
