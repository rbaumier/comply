//! security-require-rate-limit-auth backend —
//! auth-like route handlers that lack a rate-limit middleware.

use crate::diagnostic::{Diagnostic, Severity};

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

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    // Route registrations like `app.post`, `router.post`, `app.use`.
    // We only care about HTTP-verb registrations that take (path, ...handlers).
    let is_route_reg = name.ends_with(".post")
        || name.ends_with(".get")
        || name.ends_with(".put")
        || name.ends_with(".patch")
        || name.ends_with(".all");
    if !is_route_reg {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    let mut cursor = args.walk();
    let positional: Vec<_> = args
        .children(&mut cursor)
        .filter(|c| !matches!(c.kind(), "(" | ")" | ","))
        .collect();
    let Some(path_node) = positional.first() else {
        return;
    };
    if path_node.kind() != "string" {
        return;
    }
    let Ok(path_text) = path_node.utf8_text(source) else {
        return;
    };
    if !is_auth_path(path_text) {
        return;
    }

    // Scan middleware + handler args for anything looking like a rate limiter.
    let mut has_rl = false;
    for arg in positional.iter().skip(1) {
        let Ok(text) = arg.utf8_text(source) else {
            continue;
        };
        if looks_like_rate_limit(text) {
            has_rl = true;
            break;
        }
    }
    if has_rl {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Auth route {path_text} has no rate-limit middleware — attackers can brute-force credentials."
        ),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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
}
