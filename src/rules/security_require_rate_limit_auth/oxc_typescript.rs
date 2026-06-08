//! security-require-rate-limit-auth OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_auth_path(path: &str) -> bool {
    let unquoted = path.trim_matches(|c: char| c == '"' || c == '\'' || c == '`');
    let lower = unquoted.to_ascii_lowercase();
    lower.contains("/login")
        || lower.contains("/signin")
        || lower.contains("/sign-in")
        || lower.contains("/signup")
        || lower.contains("/sign-up")
        || lower.contains("/register")
        || lower.contains("/reset")
        || lower.contains("/forgot-password")
}

fn looks_like_rate_limit(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("ratelimit")
        || lower.contains("rate_limit")
        || lower.contains("rate-limit")
        || lower.contains("ratelimiter")
        || lower.contains("throttle")
        || lower.contains("slow-down")
        || lower.contains("slowdown")
}

fn has_global_rate_limit(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    for (i, _) in lower.match_indices(".use(") {
        let end = (i + 125).min(lower.len());
        let window = &lower[i..end];
        if looks_like_rate_limit(window) {
            return true;
        }
    }
    false
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

        // Check callee is a member expression like app.post, router.get, etc.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !matches!(method, "post" | "get" | "put" | "patch" | "all") {
            return;
        }

        // MSW request mocks (`http.post("*/api/...", handler)`) are in-process
        // test doubles, not a real network surface — there is nothing to
        // rate-limit. MSW's callee object is `http`.
        if let Expression::Identifier(obj) = &member.object
            && obj.name.as_str() == "http"
        {
            return;
        }

        // First argument must be a string literal with an auth path.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let path_text = match first_arg {
            Argument::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        // A leading `*` is an MSW URL wildcard, never valid Elysia route syntax.
        if path_text.starts_with('*') {
            return;
        }
        if !is_auth_path(path_text) {
            return;
        }

        // Check remaining args for rate-limit middleware.
        for arg in call.arguments.iter().skip(1) {
            let text = &ctx.source[arg.span().start as usize..arg.span().end as usize];
            if looks_like_rate_limit(text) {
                return;
            }
        }

        // Check for global rate-limit middleware.
        if has_global_rate_limit(ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Auth route \"{path_text}\" has no rate-limit middleware — attackers can brute-force credentials."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_tsx(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(src, &Check)
    }

    #[test]
    fn flags_auth_route_without_rate_limit() {
        let src = r#"app.post("/api/auth/login", handler);"#;
        assert_eq!(run_tsx(src).len(), 1);
    }

    // Regression for #236: an MSW `http.post` mock of an auth endpoint is a
    // test double, not a real route — nothing to rate-limit.
    #[test]
    fn allows_msw_http_post_mock() {
        let src = r#"
            mswServer.use(
                http.post("*/api/v1/auth/sign-in/email", () => HttpResponse.json({ token: "x" })),
            );
        "#;
        assert!(run_tsx(src).is_empty(), "{:?}", run_tsx(src));
    }

    #[test]
    fn allows_wildcard_path_on_other_callee() {
        let src = r#"server.post("*/auth/login", handler);"#;
        assert!(run_tsx(src).is_empty(), "{:?}", run_tsx(src));
    }



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_login_without_rate_limit() {
        assert_eq!(run("app.post('/login', loginHandler);").len(), 1);
    }


    #[test]
    fn flags_signup_without_rate_limit() {
        assert_eq!(run("router.post('/signup', signupHandler);").len(), 1);
    }


    #[test]
    fn allows_login_with_rate_limit() {
        assert!(run("app.post('/login', rateLimit(), loginHandler);").is_empty());
    }


    #[test]
    fn allows_login_with_throttle() {
        assert!(run("app.post('/login', throttle, loginHandler);").is_empty());
    }


    #[test]
    fn ignores_non_auth_paths() {
        assert!(run("app.post('/widgets', createWidget);").is_empty());
    }


    #[test]
    fn allows_global_rate_limit_via_app_use() {
        let src = "app.use(rateLimit({ windowMs: 60000 }));\napp.post('/login', loginHandler);";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_global_rate_limit_via_router_use() {
        let src = "router.use(rateLimit());\nrouter.post('/signup', signupHandler);";
        assert!(run(src).is_empty());
    }


    #[test]
    fn flags_without_global_rate_limit() {
        assert_eq!(run("app.post('/login', handler);").len(), 1);
    }
}
