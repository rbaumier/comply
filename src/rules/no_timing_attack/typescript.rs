//! no-timing-attack backend — direct comparison of security-sensitive values.

use crate::diagnostic::{Diagnostic, Severity};

/// Security-sensitive identifier fragments.
const SENSITIVE_WORDS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "apikey",
    "api_key",
    "apiKey",
    "auth",
    "hash",
    "digest",
    "signature",
    "hmac",
    "credential",
    "otp",
    "pin",
];

fn has_timing_attack_comparison(line: &str) -> bool {
    let has_comparison = line.contains("===")
        || line.contains("!==")
        || line.contains("==")
        || line.contains("!=");
    if !has_comparison {
        return false;
    }

    let lower = line.to_ascii_lowercase();
    for word in SENSITIVE_WORDS {
        if lower.contains(word) {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");

    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('*') {
            continue;
        }
        if has_timing_attack_comparison(trimmed) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-timing-attack".into(),
                message: "Direct comparison of a security-sensitive value — use constant-time comparison instead.".into(),
                severity: Severity::Error,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_password_comparison() {
        assert_eq!(run_on("if (password === input) {").len(), 1);
    }

    #[test]
    fn flags_token_comparison() {
        assert_eq!(run_on("if (userToken == expectedToken) {").len(), 1);
    }

    #[test]
    fn allows_non_sensitive_comparison() {
        assert!(run_on("if (name === other) {").is_empty());
    }

    #[test]
    fn allows_no_comparison() {
        assert!(run_on("const password = getPassword();").is_empty());
    }
}
