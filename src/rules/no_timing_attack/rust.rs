//! no-timing-attack backend for Rust.
//!
//! Flags direct `==` / `!=` comparison of security-sensitive values
//! (passwords, tokens, hashes). Use constant-time comparison instead.

use crate::diagnostic::{Diagnostic, Severity};

/// Security-sensitive identifier fragments.
const SENSITIVE_WORDS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "apikey",
    "api_key",
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
    let has_comparison = line.contains("==") || line.contains("!=");
    if !has_comparison {
        return false;
    }

    let lower = line.to_ascii_lowercase();
    SENSITIVE_WORDS.iter().any(|word| lower.contains(word))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "source_file" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");

    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("///") {
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
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_password_comparison() {
        assert_eq!(run_on("fn f(password: &str, input: &str) { if password == input {} }").len(), 1);
    }

    #[test]
    fn flags_token_comparison() {
        assert_eq!(run_on("fn f() { if user_token == expected_token {} }").len(), 1);
    }

    #[test]
    fn allows_non_sensitive_comparison() {
        assert!(run_on("fn f() { if name == other {} }").is_empty());
    }
}
