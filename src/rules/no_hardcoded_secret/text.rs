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
//! (AWS, GitHub, Stripe, JWT, Bearer) or a -keyed assignment
//! (API_KEY = "..."). False positives are acceptable; each one gets
//! justified with a comply-ignore comment.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "ACCESS_TOKEN",
            "AKIA",
            "API_KEY",
            "APIKEY",
            "gho_",
            "ghp_",
            "ghr_",
            "ghs_",
            "ghu_",
            "github_pat_",
            "PASSWORD",
            "rk_live_",
            "rk_test_",
            "SECRET",
            "service_account",
            "sk_live_",
            "sk_test_",
        ])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_doc_or_comment_line(line) {
                continue;
            }
            if let Some(kind) = scan_line(line) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-hardcoded-secret".into(),
                    message: format!(
                        "Possible hardcoded secret ({kind}) — move it to an \
                         environment variable or secret store. If this is a \
                         false positive, add a comply-ignore comment explaining."
                    ),
                    severity: Severity::Error,
                    span: None,
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
    // Slack token: xoxb-/xoxp-/xoxa-/xoxo- prefix.
    if contains_slack_token(line) {
        return Some("Slack token");
    }
    // Private key header (PEM / PGP).
    if contains_private_key_header(line) {
        return Some("private key");
    }
    // Slack webhook URL.
    if line.contains("hooks.slack.com/services/") {
        return Some("Slack webhook URL");
    }
    // Twilio API key: SK + 32 hex chars.
    if contains_twilio_key(line) {
        return Some("Twilio API key");
    }
    // Password in URL: ://user:password@host.
    if contains_password_in_url(line) {
        return Some("password in URL");
    }
    // GCP service account JSON.
    if contains_gcp_service_account(line) {
        return Some("GCP service account");
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
            if rest.len() >= 20
                && rest[..20]
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
            {
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
        if rest.len() >= 40
            && rest[..40]
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_')
        {
            return true;
        }
    }
    false
}

fn contains_slack_token(line: &str) -> bool {
    for prefix in ["xoxb-", "xoxp-", "xoxa-", "xoxo-"] {
        if let Some(idx) = line.find(prefix) {
            let rest = &line.as_bytes()[idx + prefix.len()..];
            if rest.len() >= 10
                && rest[..10]
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
            {
                return true;
            }
        }
    }
    false
}

fn contains_private_key_header(line: &str) -> bool {
    const HEADERS: &[&str] = &[
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----",
        "-----BEGIN DSA PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----",
        "-----BEGIN PGP PRIVATE KEY BLOCK-----",
    ];
    HEADERS.iter().any(|h| line.contains(h))
}

fn contains_twilio_key(line: &str) -> bool {
    let bytes = line.as_bytes();
    for i in 0..bytes.len().saturating_sub(34) {
        if bytes[i] == b'S'
            && bytes[i + 1] == b'K'
            && bytes[i + 2..i + 34].iter().all(|b| b.is_ascii_hexdigit())
        {
            // Ensure the char after the 34-char match is NOT hex (exact 32 hex after SK).
            if i + 34 >= bytes.len() || !bytes[i + 34].is_ascii_hexdigit() {
                return true;
            }
        }
    }
    false
}

const WELL_KNOWN_TEST_CREDENTIALS: &[&str] = &[
    "postgres:postgres@localhost",
    "root:root@localhost",
    "admin:admin@localhost",
    "user:password@localhost",
    "sa:sa@localhost",
];

fn contains_password_in_url(line: &str) -> bool {
    let Some(proto_end) = line.find("://") else {
        return false;
    };
    let after = &line[proto_end + 3..];
    if let Some(colon) = after.find(':') {
        let rest = &after[colon + 1..];
        if let Some(at) = rest.find('@')
            && at > 0 && !after[..colon].contains('/') {
                let credentials_and_host = &after[..colon
                    + 1
                    + at
                    + 1
                    + rest[at + 1..]
                        .find([':', '/'])
                        .unwrap_or(rest.len() - at - 1)];
                if WELL_KNOWN_TEST_CREDENTIALS
                    .iter()
                    .any(|c| credentials_and_host.contains(c))
                {
                    return false;
                }
                return true;
            }
    }
    false
}

fn is_doc_or_comment_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("///")
        || trimmed.starts_with("//!")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("*\t")
        || trimmed.starts_with("/**")
}

fn contains_gcp_service_account(line: &str) -> bool {
    let normalized: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    normalized.contains("\"type\":\"service_account\"")
}

/// Detect `CONST_NAME = "long-literal"` where CONST_NAME contains a secret-ish word.
fn contains_keyed_literal(line: &str) -> bool {
    const KEYS: &[&str] = &["SECRET", "PASSWORD", "API_KEY", "APIKEY", "ACCESS_TOKEN"];
    // Require an `=` followed by a quoted string at least 16 chars long.
    let Some(eq_pos) = line.find('=') else {
        return false;
    };
    // The sensitive keyword must appear in the LEFT side (the variable/key name),
    // not in the value. This avoids FPs on `autoComplete="current-password"` or
    // `to="/forgot-password"` where the keyword is in the assigned value.
    let left = &line[..eq_pos].to_ascii_uppercase();
    if !KEYS.iter().any(|k| left.contains(k)) {
        return false;
    }
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
        assert_eq!(run("const API_KEY = 'abcd1234567890abcdef';").len(), 1);
    }

    #[test]
    fn allows_env_var_reference() {
        assert!(run("const API_KEY = process.env.API_KEY;").is_empty());
    }

    #[test]
    fn allows_template_literal_with_interpolation() {
        assert!(run("const API_KEY = `${process.env.KEY}`;").is_empty());
    }

    #[test]
    fn flags_slack_token() {
        assert_eq!(run("const token = 'xoxb-1234567890-abcdefghij';").len(), 1);
    }

    #[test]
    fn flags_private_key_header() {
        assert_eq!(run("const k = '-----BEGIN RSA PRIVATE KEY-----';").len(), 1);
        assert_eq!(run("-----BEGIN OPENSSH PRIVATE KEY-----").len(), 1);
    }

    #[test]
    fn flags_slack_webhook() {
        assert_eq!(
            run("const url = 'https://hooks.slack.com/services/T00/B00/xxxx';").len(),
            1
        );
    }

    #[test]
    fn flags_twilio_key() {
        assert_eq!(
            run("const key = 'SK1234567890abcdef1234567890abcdef';").len(),
            1
        );
    }

    #[test]
    fn allows_sk_with_wrong_length() {
        // SK followed by 31 hex chars (not 32) — should not match.
        assert!(run("const key = 'SK1234567890abcdef1234567890abcde';").is_empty());
    }

    #[test]
    fn flags_password_in_url() {
        assert_eq!(
            run("const db = 'postgres://admin:s3cret@localhost:5432/db';").len(),
            1
        );
    }

    #[test]
    fn allows_url_without_password() {
        assert!(run("const url = 'https://example.com/path';").is_empty());
    }

    #[test]
    fn flags_gcp_service_account() {
        assert_eq!(run(r#""type": "service_account""#).len(), 1);
        assert_eq!(run(r#""type":"service_account""#).len(), 1);
    }

    #[test]
    fn allows_html_attribute_with_password_in_value() {
        assert!(run(r#"autoComplete="current-password""#).is_empty());
    }

    #[test]
    fn allows_route_path_with_password_in_value() {
        assert!(run(r#"to="/forgot-password""#).is_empty());
    }

    #[test]
    fn allows_i18n_key_with_key_in_namespace() {
        assert!(run(r#"t("apiKeys.title")"#).is_empty());
    }

    #[test]
    fn allows_doc_comment_with_url_example() {
        assert!(run(r#"/// `smtps://user:password@provider.com:465`"#).is_empty());
    }

    #[test]
    fn allows_postgres_test_default() {
        assert!(
            run(r#"const db = "postgres://postgres:postgres@localhost:5432/test";"#).is_empty()
        );
    }

    #[test]
    fn still_flags_real_password_in_url() {
        assert_eq!(
            run(r#"const db = "postgres://admin:s3cretProd@db.example.com:5432/prod";"#).len(),
            1
        );
    }
}
