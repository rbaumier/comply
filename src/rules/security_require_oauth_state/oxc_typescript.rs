//! security-require-oauth-state OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn validates_state(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.contains("state===")
        || compact.contains("state!==")
        || compact.contains("state==")
        || compact.contains("state!=")
        || compact.contains("===state")
        || compact.contains("!==state")
        || compact.contains("==state")
        || compact.contains("!=state")
    {
        return true;
    }
    if compact.contains(".state")
        || compact.contains("[\"state\"]")
        || compact.contains("['state']")
    {
        return true;
    }
    if compact.contains(".get(\"state\")") || compact.contains(".get('state')") {
        return true;
    }
    let lower = compact.to_ascii_lowercase();
    if lower.contains("verifystate")
        || lower.contains("validatestate")
        || lower.contains("checkstate")
        || lower.contains("assertstate")
    {
        return true;
    }
    for marker in ["verify(", "validate(", "check(", "assert("] {
        if let Some(idx) = lower.find(marker) {
            let tail = &lower[idx + marker.len()..];
            if tail.starts_with("state") {
                return true;
            }
        }
    }
    false
}

fn is_oauth_callback_path(path: &str) -> bool {
    let unquoted = path.trim_matches(|c: char| c == '"' || c == '\'' || c == '`');
    let lower = unquoted.to_ascii_lowercase();
    // Drop a query string and trailing slash so the terminal-segment check is robust.
    let stem = lower.split('?').next().unwrap_or(&lower).trim_end_matches('/');
    // Explicit OAuth/auth context anywhere covers provider sub-paths like
    // `/auth/callback/:provider`.
    if stem.contains("/oauth/callback")
        || stem.contains("/oauth2/callback")
        || stem.contains("/auth/callback")
        || stem.contains("/sso/callback")
    {
        return true;
    }
    // A bare `/callback` (or `/cb`) is an OAuth redirect URI only as the terminal
    // route segment; `/callback/request`, `/callback/response` are sub-routes.
    matches!(stem.rsplit('/').next(), Some("callback" | "cb"))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `.get`, `.post`, or `.all`
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !matches!(method, "get" | "post" | "all") {
            return;
        }

        // First argument must be a string with an OAuth callback path
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let path_text = match first_arg {
            oxc_ast::ast::Argument::StringLiteral(s) => {
                let raw = &ctx.source[s.span.start as usize..s.span.end as usize];
                raw.to_string()
            }
            _ => return,
        };
        if !is_oauth_callback_path(&path_text) {
            return;
        }

        // Check handler arguments (skip first arg = path) for state validation
        let mut reads_state = false;
        for arg in call.arguments.iter().skip(1) {
            let span = arg.span();
            let text = &ctx.source[span.start as usize..span.end as usize];
            let stripped = strip_comments(text);
            if validates_state(&stripped) {
                reads_state = true;
                break;
            }
        }
        if reads_state {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "OAuth callback handler {path_text} never reads `state` — CSRF validation is missing."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // #3225: AWS Lambda@Edge adapter test routes — `/callback/request` and
    // `/callback/response` — are not OAuth callbacks. They flagged under the old
    // bare `/callback` substring match; the terminal-segment rule exempts them.
    #[test]
    fn ignores_lambda_callback_sub_routes() {
        let src = r#"
            app.get('/callback/request', async (c, next) => {
              await next()
              c.env.callback(null, c.env.request)
            })
            app.get('/callback/response', async (c, next) => {
              await next()
              c.env.callback(null, c.env.response)
            })
        "#;
        assert!(run(src).is_empty());
    }

    // Security detection preserved: a bare `/callback` OAuth handler that never
    // reads `state` is the canonical CSRF target and must still flag.
    #[test]
    fn flags_bare_callback_without_state() {
        let src = r#"app.get('/callback', (c) => { return c.text('ok') })"#;
        assert_eq!(run(src).len(), 1);
    }

    // `/auth/callback/:provider`-style provider sub-paths keep firing because the
    // explicit auth context matches anywhere.
    #[test]
    fn flags_provider_sub_path() {
        let src = r#"app.get('/auth/callback/google', (c) => { return c.text('ok') })"#;
        assert_eq!(run(src).len(), 1);
    }

    // Unit coverage for the path predicate across the cases the route harness
    // can't reach as terminal route literals.
    #[test]
    fn callback_path_classification() {
        // FP fixed: sub-routes under a `callback` segment are not OAuth endpoints.
        assert!(!is_oauth_callback_path("/callback/request"));
        assert!(!is_oauth_callback_path("/callback/response"));
        // Bonus precision: a plural `callbacks` terminal segment is not a callback.
        assert!(!is_oauth_callback_path("/callbacks"));

        // Security detection preserved.
        assert!(is_oauth_callback_path("/callback"));
        assert!(is_oauth_callback_path("/auth/callback"));
        assert!(is_oauth_callback_path("/oauth/callback"));
        assert!(is_oauth_callback_path("/oauth2/callback"));
        assert!(is_oauth_callback_path("/sso/callback"));
        assert!(is_oauth_callback_path("/cb"));
        // Explicit auth context still matches as a non-terminal prefix.
        assert!(is_oauth_callback_path("/auth/callback/google"));
        // Trailing slash and query string don't defeat the terminal-segment check.
        assert!(is_oauth_callback_path("/callback/"));
        assert!(is_oauth_callback_path("/callback?code=abc"));
    }
}
