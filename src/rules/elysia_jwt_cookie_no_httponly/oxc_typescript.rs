use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const AUTH_PATTERNS: &[&str] = &["session", "token", "jwt", "auth", "sid"];

fn is_auth_cookie_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    // Split on word separators to avoid matching "sid" inside "sidebar_state"
    let segments: Vec<&str> = lower.split(|c| c == '_' || c == '-').collect();
    AUTH_PATTERNS.iter().any(|&p| {
        segments.iter().any(|s| {
            if p.len() <= 3 {
                // Short patterns (sid, jwt) require exact segment match
                *s == p
            } else {
                // Longer patterns (session, token, auth) allow substring within a segment
                s.contains(p)
            }
        })
    })
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
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
            if !line.contains(".set({") {
                continue;
            }
            if let Some(pos) = line.find(".set({") {
                let before = &line[..pos];
                if !before.contains("cookie") {
                    continue;
                }
                let cookie_name = before.split('.').filter(|s| !s.is_empty()).last().unwrap_or("");
                if !is_auth_cookie_name(cookie_name) {
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
                path: Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
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
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_auth_cookie_without_httponly() {
        let src = "cookie.auth.set({ value: token, secure: true });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_session_cookie_without_httponly() {
        let src = "cookie.session.set({ value: id });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_jwt_cookie_without_httponly() {
        let src = "cookie.jwt.set({ value: token });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_auth_cookie_with_httponly() {
        let src = "cookie.auth.set({ value: token, httpOnly: true, secure: true });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_on_ui_state_cookie() {
        // sidebar_state is a UI preference cookie — must be JS-readable, not an auth token
        let src = "cookie.sidebar_state.set({ value: String(open) });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_on_theme_cookie() {
        let src = "cookie.theme.set({ value: 'dark', path: '/' });";
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
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
