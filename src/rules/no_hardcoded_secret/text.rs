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

impl Check {
    /// Literal substrings that gate the rule — shared by the text backend and
    /// the Rust tree-sitter backend so both prefilter on the same token set.
    pub(crate) const PREFILTER: &'static [&'static str] = &[
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
    ];
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(Self::PREFILTER)
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
pub(crate) fn scan_line(line: &str) -> Option<&'static str> {
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
    "root:password@localhost",
    "postgres:password@localhost",
];

fn contains_password_in_url(line: &str) -> bool {
    let Some(proto_end) = line.find("://") else {
        return false;
    };
    let after = &line[proto_end + 3..];
    if let Some(colon) = after.find(':') {
        let rest = &after[colon + 1..];
        if let Some(at) = rest.find('@')
            && at > 0 && !after[..colon].contains('/')
            // A bracketed token in the userinfo (`[[user]:[password]@]`) marks a
            // URL-format template in documentation, not a real credential —
            // a real userinfo component would percent-encode `[`/`]`.
            && !after[..colon + 1 + at].contains('[') {
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

pub(crate) fn is_doc_or_comment_line(line: &str) -> bool {
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
    // Name-based exemptions operate on the assigned identifier in original case
    // (e.g. `secretEndpoint`, `API_KEY_HEADER_NAME`), so extract it before the
    // left side is consumed as uppercase.
    let name = assigned_name(&line[..eq_pos]);
    if names_an_identifier_not_a_value(name) || is_secret_as_adjective(name) {
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
    if inner.len() < 16 || inner.contains("${") {
        return false;
    }
    // Values prefixed with `test_` or `fake_` are deliberately fabricated test
    // credentials (e.g. Vitest setup files that satisfy schema validation without
    // real secrets, or fixtures named `fake_secret_info`).
    let inner_lower = inner.to_ascii_lowercase();
    if inner_lower.starts_with("test_") || inner_lower.starts_with("fake_") {
        return false;
    }
    // Placeholder notations from documentation and samples are never real
    // credentials: angle-bracket tokens (`<another-client-secret>`) and
    // asterisk-masked text (`***Access Token***`).
    if is_placeholder_literal(&inner) {
        return false;
    }
    // Attribute-name constants hold symbolic keys (database field names, protocol
    // parameters) rather than actual credentials. The variable name mirrors the
    // value: ATTR_APPLICATION_PASSWORD holds "application_password". Detect this
    // by checking whether the variable name (left side, uppercased) contains the
    // value (uppercased), which is impossible for random credential strings.
    // URN constants (e.g. "urn:ietf:params:oauth:token-type:access_token") are
    // exempt separately because their colons make them recognisable as protocol
    // identifiers rather than secret material.
    let left_upper = left.to_ascii_uppercase();
    let value_upper = inner.to_ascii_uppercase().replace(['-', ':', '/', '.'], "_");
    if left_upper.contains(&value_upper) {
        return false;
    }
    if is_urn_or_protocol_identifier(&inner) {
        return false;
    }
    true
}

/// Extract the assigned identifier from the left side of an assignment, in
/// original case. Returns the last identifier-like run (alphanumeric or `_`),
/// which covers `const secretEndpoint`, `API_KEY_HEADER_NAME`, and bracket-key
/// forms like `process.env["ACCESS_TOKEN"]`.
fn assigned_name(left: &str) -> &str {
    let bytes = left.as_bytes();
    let mut end = bytes.len();
    while end > 0 && !is_ident_byte(bytes[end - 1]) {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    &left[start..end]
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Names ending in `_NAME`, `_HEADER`, or `_HEADER_NAME` hold a symbolic
/// identifier (an HTTP header name, a field key) rather than a credential
/// value — e.g. `API_KEY_HEADER_NAME = "subscription-key"`.
fn names_an_identifier_not_a_value(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.ends_with("_NAME") || upper.ends_with("_HEADER")
}

/// `secret` used as an adjective ("the X to be kept confidential") rather than
/// a noun ("a value that is a secret"): `secretEndpoint`, `secretUrl`,
/// `secretPath`, etc. The value such a variable holds is a location, not a key.
fn is_secret_as_adjective(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("secret") else {
        return false;
    };
    matches!(
        rest,
        "Endpoint" | "Url" | "Uri" | "Path" | "Host" | "Hostname" | "Domain" | "File" | "Dir"
    )
}

/// Documentation placeholders: `<angle-bracket>` tokens or `***masked***`
/// values. Never real credentials.
fn is_placeholder_literal(s: &str) -> bool {
    let trimmed = s.trim();
    (trimmed.starts_with('<') && trimmed.ends_with('>'))
        || (trimmed.starts_with("***") && trimmed.ends_with("***"))
}

/// Returns true when the value is a URN or protocol identifier — a colon-
/// delimited path used as an OAuth2 token type, SAML binding, or similar.
/// These are symbolic keys, never secret material.
fn is_urn_or_protocol_identifier(s: &str) -> bool {
    s.contains(':')
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-' | ':' | '/' | '.'))
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

    #[test]
    fn allows_test_prefixed_secret_in_vitest_setup() {
        // Vitest setup files inject fake secrets with a test_ prefix to satisfy
        // schema validation without using real credentials (Closes #505).
        assert!(
            run(r#"process.env['API_AUTH_SECRET'] = 'test_secret_padded_to_meet_min_length_xx';"#)
                .is_empty()
        );
    }

    #[test]
    fn still_flags_real_secret_without_test_prefix() {
        assert_eq!(
            run(r#"const API_AUTH_SECRET = 'prod_secret_padded_to_meet_min_length_xx';"#).len(),
            1
        );
    }

    // Regression tests for #985 — attribute-name constants (Closes #985)
    #[test]
    fn allows_attr_name_constants_with_password_keyword() {
        // Database attribute name — value is a symbolic key, not a credential.
        assert!(run(r#"pub const ATTR_APPLICATION_PASSWORD: &str = "application_password";"#).is_empty());
        assert!(run(r#"pub const ATTR_BADLIST_PASSWORD: &str = "badlist_password";"#).is_empty());
    }

    #[test]
    fn allows_attr_name_constants_with_secret_keyword() {
        assert!(run(r#"pub const ATTR_OAUTH2_RS_BASIC_SECRET: &str = "oauth2_rs_basic_secret";"#).is_empty());
        assert!(run(r#"pub const ATTR_OAUTH2_CLIENT_SECRET: &str = "oauth2_client_secret";"#).is_empty());
    }

    #[test]
    fn allows_urn_access_token_constant() {
        // RFC URI constant — symbolic protocol parameter, not a credential.
        assert!(run(r#"pub const OAUTH2_TOKEN_TYPE_ACCESS_TOKEN: &str = "urn:ietf:params:oauth:token-type:access_token";"#).is_empty());
    }

    #[test]
    fn allows_api_token_session_constant() {
        assert!(run(r#"pub const ATTR_API_TOKEN_SESSION: &str = "api_token_session";"#).is_empty());
    }

    #[test]
    fn still_flags_mixed_case_value_with_secret_keyword() {
        // Mixed-case or base62 value — genuine credential shape.
        assert_eq!(
            run(r#"const CLIENT_SECRET = "Abc123XYZqwertyuiop";"#).len(),
            1
        );
    }

    // Regression tests for #1065 — azure-sdk-for-js FP patterns (Closes #1065)
    #[test]
    fn allows_header_name_constant() {
        // The variable NAMES the header; it holds an identifier, not a key.
        assert!(run(r#"const API_KEY_HEADER_NAME = "subscription-key";"#).is_empty());
    }

    #[test]
    fn allows_fake_prefixed_value() {
        // `fake_` is a deliberately fabricated fixture, parallel to `test_`.
        assert!(run(r#"const fakeSecretValue = "fake_secret_info";"#).is_empty());
    }

    #[test]
    fn allows_angle_bracket_placeholder() {
        assert!(run(r#"const anotherSecret = "<another-client-secret>";"#).is_empty());
    }

    #[test]
    fn allows_asterisk_masked_placeholder() {
        assert!(run(r#"process.env["ACCESS_TOKEN"] = "***Access Token***";"#).is_empty());
    }

    #[test]
    fn allows_secret_as_adjective_on_endpoint() {
        // `secretEndpoint` = "the endpoint to keep confidential", not a key value.
        assert!(run(r#"const secretEndpoint = "host.docker.internal";"#).is_empty());
    }

    #[test]
    fn still_flags_real_secret_after_1065_exemptions() {
        // A genuine credential assignment must continue to fire.
        assert_eq!(run(r#"const API_KEY = "abcd1234567890abcdef";"#).len(), 1);
        assert_eq!(run(r#"const CLIENT_SECRET = "Abc123XYZqwertyuiop";"#).len(), 1);
    }

    // Regression tests for #2078 — canonical MySQL/PostgreSQL dev placeholders.
    #[test]
    fn allows_mysql_root_password_localhost_placeholder() {
        assert!(
            run(r#"DATABASE_URL="mysql://root:password@localhost:3306/myapp""#).is_empty()
        );
    }

    #[test]
    fn allows_postgres_password_localhost_placeholder() {
        assert!(
            run(r#"DATABASE_URL="postgresql://postgres:password@localhost:5432/myapp""#)
                .is_empty()
        );
    }

    #[test]
    fn still_flags_non_localhost_db_password() {
        assert_eq!(
            run(r#"const db = "mysql://admin:s3cretProd@db.prod.example.com:3306/app";"#).len(),
            1
        );
    }

    // Regression test for #1495 (pattern 1) — a bracketed URL-format template
    // placeholder in an error/documentation string is not a real credential.
    #[test]
    fn allows_bracketed_url_template_placeholder() {
        assert!(
            run(r#"let msg = "URLs must be in the form `mysql://[[user]:[password]@]host[:port][/database]`";"#)
                .is_empty()
        );
    }
}
