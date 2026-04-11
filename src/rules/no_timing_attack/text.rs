use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Security-sensitive identifier fragments. If any side of `===`/`==`/`!==`/`!=`
/// contains one of these, flag it as a potential timing attack.
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
    // Look for `===`, `==`, `!==`, `!=` in the line.
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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
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
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_password_comparison() {
        assert_eq!(run("if (password === input) {").len(), 1);
    }

    #[test]
    fn flags_token_comparison() {
        assert_eq!(run("if (userToken == expectedToken) {").len(), 1);
    }

    #[test]
    fn flags_api_key_comparison() {
        assert_eq!(run("return apiKey !== storedKey;").len(), 1);
    }

    #[test]
    fn allows_non_sensitive_comparison() {
        assert!(run("if (name === other) {").is_empty());
    }

    #[test]
    fn allows_no_comparison() {
        assert!(run("const password = getPassword();").is_empty());
    }
}
