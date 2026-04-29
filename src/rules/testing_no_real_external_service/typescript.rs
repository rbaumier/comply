//! testing-no-real-external-service backend — flag `fetch`/`axios`
//! calls to real external URLs from test files.
//!
//! Why: a test that reaches out to `https://api.stripe.com` or similar
//! depends on network, external keys, and third-party uptime. CI flakes
//! and the test suite becomes useless. Intercept with MSW instead.
//!
//! Matches tree-sitter `call_expression` nodes whose callee is `fetch`
//! or `axios.<method>` and whose first string argument contains one of
//! a closed list of banned external domains.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

const BANNED_DOMAINS: &[&str] = &[
    "stripe.com",
    "api.stripe.com",
    "api.sendgrid.com",
    "sendgrid.com",
    "api.twilio.com",
    "twilio.com",
    "api.openai.com",
    "openai.com",
    "api.anthropic.com",
    "anthropic.com",
    "api.github.com",
    "slack.com",
    "hooks.slack.com",
    "api.mailgun.net",
    "mailgun.net",
    "sentry.io",
    "ingest.sentry.io",
];

const AXIOS_METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "request", "head"];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

fn unquote(raw: &str) -> &str {
    raw.trim_start_matches(['\'', '"', '`'])
        .trim_end_matches(['\'', '"', '`'])
}

/// Is `node`'s callee a bare `fetch` identifier?
fn is_fetch_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(func) = node.child_by_field_name("function") else { return false };
    if func.kind() != "identifier" { return false }
    func.utf8_text(source).unwrap_or("") == "fetch"
}

/// Is `node`'s callee `axios.<http-method>` or `axios(...)`?
fn is_axios_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(func) = node.child_by_field_name("function") else { return false };
    match func.kind() {
        "identifier" => func.utf8_text(source).unwrap_or("") == "axios",
        "member_expression" => {
            let Some(obj) = func.child_by_field_name("object") else { return false };
            let Some(prop) = func.child_by_field_name("property") else { return false };
            if obj.utf8_text(source).unwrap_or("") != "axios" { return false }
            AXIOS_METHODS.contains(&prop.utf8_text(source).unwrap_or(""))
        }
        _ => false,
    }
}

crate::ast_check! { on ["call_expression"] prefilter = ["api.stripe.com", "api.openai.com", "api.anthropic.com", "api.github.com", "api.sendgrid.com", "api.mailgun.net", "api.twilio.com"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) { return; }
    if !(is_fetch_call(node, source) || is_axios_call(node, source)) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first) = args.named_child(0) else { return };
    if !matches!(first.kind(), "string" | "template_string") { return; }
    let raw = first.utf8_text(source).unwrap_or("");
    let url = unquote(raw);

    if BANNED_DOMAINS.iter().any(|d| url.contains(d)) {
        let pos = first.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "testing-no-real-external-service".into(),
            message: "Test makes a real network call to an external service — intercept it with MSW instead of hitting the live endpoint.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(path: &str, s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(s, &Check, path)
    }

    #[test]
    fn flags_fetch_to_stripe() {
        assert_eq!(
            run("a.test.ts", "await fetch('https://api.stripe.com/v1/charges');").len(),
            1
        );
    }

    #[test]
    fn flags_axios_get_to_openai() {
        assert_eq!(
            run("a.spec.ts", "const r = axios.get('https://api.openai.com/v1/chat');").len(),
            1
        );
    }

    #[test]
    fn flags_axios_post_to_sendgrid() {
        assert_eq!(
            run("a.test.ts", "await axios.post('https://api.sendgrid.com/v3/mail/send', body);").len(),
            1
        );
    }

    #[test]
    fn allows_localhost() {
        assert!(run("a.test.ts", "fetch('http://localhost:3000/api');").is_empty());
    }

    #[test]
    fn allows_internal_relative_url() {
        assert!(run("a.test.ts", "fetch('/api/users');").is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        assert!(run("utils.ts", "fetch('https://api.stripe.com/v1/charges');").is_empty());
    }
}
