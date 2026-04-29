//! no-auth-token-in-localstorage backend — flag
//! `localStorage.setItem('token' | 'jwt' | 'authToken' | ...)`.
//!
//! Why: localStorage is readable by any JavaScript on the page, which
//! means XSS (even a single successful one anywhere in your app) exfiltrates
//! the auth token. The correct storage for session tokens is an httpOnly
//! cookie the browser attaches automatically — JS can't read it, so XSS
//! can't steal it.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const TOKEN_KEYS: &[&str] = &[
    "token",
    "jwt",
    "authtoken",
    "accesstoken",
    "refreshtoken",
    "bearer",
    "apikey",
    "api_key",
    "session",
    "sessiontoken",
    "idtoken",
    "id_token",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["localStorage", "sessionStorage"])
    }

    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["call_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let Ok(fn_text) = function.utf8_text(source_bytes) else {
            return;
        };
        if fn_text != "localStorage.setItem" && fn_text != "sessionStorage.setItem" {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let Some(key_arg) = args.named_child(0) else {
            return;
        };
        let Ok(key_text) = key_arg.utf8_text(source_bytes) else {
            return;
        };
        let normalized = key_text
            .trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .to_ascii_lowercase()
            .replace('-', "");
        if !TOKEN_KEYS.iter().any(|t| normalized.contains(t)) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-auth-token-in-localstorage".into(),
            message: format!(
                "Storing '{key_text}' in {fn_text} — XSS exfiltrates it. \
                 Use an httpOnly cookie instead: the browser attaches it \
                 automatically, JavaScript can't read it, XSS can't steal it."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_token_storage() {
        assert_eq!(
            run_on("localStorage.setItem('authToken', t);").len(),
            1
        );
    }

    #[test]
    fn flags_jwt_storage() {
        assert_eq!(run_on("localStorage.setItem('jwt', t);").len(), 1);
    }

    #[test]
    fn flags_session_storage() {
        assert_eq!(
            run_on("sessionStorage.setItem('sessionToken', t);").len(),
            1
        );
    }

    #[test]
    fn allows_non_token_key() {
        assert!(run_on("localStorage.setItem('theme', 'dark');").is_empty());
    }
}
