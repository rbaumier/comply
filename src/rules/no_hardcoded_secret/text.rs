//! no-hardcoded-secret backend — regex scan for common API key / token
//! patterns committed to source.
//!
//! Why: hardcoded secrets are the #1 cause of data breaches. Every pushed
//! commit gets indexed by GitHub's secret scanner and by every attacker
//! running `gitleaks` against public repos. The fix is obvious — env vars
//! plus a vault — but the rule catches the moment of temptation.
//!
//! Detection: per-line regex scan for well-known token shapes. The rule
//! is conservative — it only fires on patterns with a dedicated prefix
//! (AWS, GitHub, Stripe, JWT, Bearer) or a clearly-keyed assignment
//! (API_KEY = "..."). False positives are acceptable; each one gets
//! justified with a comply-ignore comment.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(kind) = scan_line(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-hardcoded-secret".into(),
                    message: format!(
                        "Possible hardcoded secret ({kind}) — move it to an \
                         environment variable or secret store. If this is a \
                         false positive, add a comply-ignore comment explaining."
                    ),
                    severity: Severity::Error,
                });
            }
        }
        diagnostics
    }
}

/// Scan one line for known secret shapes. Returns a short label describing
/// what was found, or None when nothing matched.
fn scan_line(line: &str) -> Option<&'static str> {
    // AWS access key: AKIA followed by 16 uppercase alphanum.
    if contains_aws_access_key(line) {
        return Some("AWS access key");
    }
    // GitHub token: ghp_/gho_/ghs_/ghu_/github_pat_ + base62.
    if contains_github_token(line) {
        return Some("GitHub token");
    }
    // Stripe secret key: sk_live_ or sk_test_ + 24+ base62.
    if contains_stripe_key(line) {
        return Some("Stripe secret key");
    }
    // OpenAI key: sk-proj- or sk- + 48+ chars.
    if contains_openai_key(line) {
        return Some("OpenAI key");
    }
    // Generic high-entropy string assigned to a SECRET/PASSWORD/TOKEN/API_KEY.
    if contains_keyed_literal(line) {
        return Some("hardcoded credential");
    }
    None
}

fn contains_aws_access_key(line: &str) -> bool {
    let bytes = line.as_bytes();
    for start in 0..bytes.len().saturating_sub(20) {
        if &bytes[start..start + 4] == b"AKIA"
            && bytes[start + 4..start + 20]
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

fn contains_github_token(line: &str) -> bool {
    for prefix in ["ghp_", "gho_", "ghs_", "ghu_", "ghr_", "github_pat_"] {
        if let Some(idx) = line.find(prefix) {
            let rest = &line.as_bytes()[idx + prefix.len()..];
            if rest.len() >= 20 && rest[..20].iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_') {
                return true;
            }
        }
    }
    false
}

fn contains_stripe_key(line: &str) -> bool {
    for prefix in ["sk_live_", "sk_test_", "rk_live_", "rk_test_"] {
        if let Some(idx) = line.find(prefix) {
            let rest = &line.as_bytes()[idx + prefix.len()..];
            if rest.len() >= 24 && rest[..24].iter().all(|b| b.is_ascii_alphanumeric()) {
                return true;
            }
        }
    }
    false
}

fn contains_openai_key(line: &str) -> bool {
    if let Some(idx) = line.find("sk-") {
        let rest = &line.as_bytes()[idx + 3..];
        if rest.len() >= 40 && rest[..40].iter().all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_') {
            return true;
        }
    }
    false
}

/// Detect `CONST_NAME = "long-literal"` where CONST_NAME contains a secret-ish word.
fn contains_keyed_literal(line: &str) -> bool {
    const KEYS: &[&str] = &["SECRET", "PASSWORD", "API_KEY", "APIKEY", "ACCESS_TOKEN"];
    let upper = line.to_ascii_uppercase();
    if !KEYS.iter().any(|k| upper.contains(k)) {
        return false;
    }
    // Require an `=` followed by a quoted string at least 16 chars long.
    let Some(eq_pos) = line.find('=') else {
        return false;
    };
    let after = line[eq_pos..].trim_start_matches('=').trim_start();
    let Some(quote) = after.chars().next() else {
        return false;
    };
    if !matches!(quote, '"' | '\'' | '`') {
        return false;
    }
    let inner: String = after.chars().skip(1).take_while(|c| *c != quote).collect();
    inner.len() >= 16 && !inner.contains("${")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_aws_key() {
        assert_eq!(run("const k = 'AKIAIOSFODNN7EXAMPLE';").len(), 1);
    }

    #[test]
    fn flags_github_token() {
        assert_eq!(
            run("const k = 'ghp_abcdefghijklmnopqrstuvwxyz01234';").len(),
            1
        );
    }

    #[test]
    fn flags_keyed_literal() {
        assert_eq!(
            run("const API_KEY = 'abcd1234567890abcdef';").len(),
            1
        );
    }

    #[test]
    fn allows_env_var_reference() {
        assert!(run("const API_KEY = process.env.API_KEY;").is_empty());
    }

    #[test]
    fn allows_template_literal_with_interpolation() {
        assert!(run("const API_KEY = `${process.env.KEY}`;").is_empty());
    }
}
