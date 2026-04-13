use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns true if a string literal (between quotes) looks like a secret:
/// alphanumeric (with common secret chars like +/=_-) and longer than 8 chars.
fn looks_like_secret(s: &str) -> bool {
    s.len() > 8
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '_' | '-'))
}

/// Extract the content between the first pair of quotes (single or double) after `start`.
fn extract_quoted_string(line: &str, start: usize) -> Option<&str> {
    let rest = &line[start..];
    for quote in ['"', '\''] {
        if let Some(open) = rest.find(quote) {
            let after_open = open + 1;
            if let Some(close) = rest[after_open..].find(quote) {
                return Some(&rest[after_open..after_open + close]);
            }
        }
    }
    None
}

fn has_hardcoded_secret(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();

    // Look for .sign( or .verify( calls
    for func in [".sign(", ".verify("] {
        let mut search_from = 0;
        while let Some(pos) = lower[search_from..].find(func) {
            let abs = search_from + pos + func.len();
            // Look for a string literal argument (the secret/key) in the arguments
            if let Some(secret) = extract_quoted_string(line, abs)
                && looks_like_secret(secret) {
                    return true;
                }
            search_from = abs;
        }
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_hardcoded_secret(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-hardcoded-secret-signature".into(),
                    message:
                        "Hardcoded secret in signing/verification — use env vars or a secrets manager."
                            .into(),
                    severity: Severity::Error,
                    span: None,
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
    fn flags_jwt_sign_with_hardcoded_secret() {
        assert_eq!(
            run("const token = jwt.sign(payload, 'mySuperSecretKey123');").len(),
            1
        );
    }

    #[test]
    fn flags_verify_with_hardcoded_secret() {
        assert_eq!(
            run("const decoded = jwt.verify(token, 'aVeryLongSecretString');").len(),
            1
        );
    }

    #[test]
    fn allows_sign_with_variable() {
        assert!(run("const token = jwt.sign(payload, process.env.SECRET);").is_empty());
    }

    #[test]
    fn allows_sign_with_short_string() {
        // Short strings (<=8 chars) are not flagged — likely not secrets
        assert!(run("const token = jwt.sign(payload, 'test');").is_empty());
    }

    #[test]
    fn ignores_non_crypto_sign() {
        assert!(run("document.sign('hello');").is_empty());
    }
}
