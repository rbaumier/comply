//! security-require-pkce-oauth backend —
//! flag string literals building an OAuth /authorize URL without `code_challenge`.

use crate::diagnostic::{Diagnostic, Severity};

fn looks_like_authorize_url(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    (lower.contains("/authorize") || lower.contains("/oauth/authorize") || lower.contains("/auth?"))
        && (lower.contains("client_id") || lower.contains("response_type"))
}

crate::ast_check! { on ["string", "template_string"] => |node, source, ctx, diagnostics|
    // Detect string or template_string literals that build an OAuth authorize URL.
    let Ok(text) = node.utf8_text(source) else {
        return;
    };
    if !looks_like_authorize_url(text) {
        return;
    }
    // If `code_challenge` is already present in the literal, we're fine.
    if text.contains("code_challenge") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "OAuth authorize URL is missing `code_challenge` — PKCE is required for public clients.".into(),
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
    fn flags_authorize_url_without_pkce() {
        let src = "const url = 'https://idp.example.com/oauth/authorize?client_id=abc&response_type=code';";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_authorize_url_with_pkce() {
        let src = "const url = 'https://idp.example.com/oauth/authorize?client_id=abc&response_type=code&code_challenge=xyz&code_challenge_method=S256';";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_unrelated_strings() {
        assert!(run("const s = 'hello world';").is_empty());
    }
}
