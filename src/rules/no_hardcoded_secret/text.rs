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
                // Test suites embed key blocks as fixtures: crypto libraries
                // commit armored PGP key pairs as deterministic test vectors,
                // and GitHub Apps SDKs embed a purpose-generated RSA/EC PEM key
                // to exercise JWT signing against mock servers. Inside a test
                // directory such a key block is fixture data, not a leaked
                // production secret. Token-prefix shapes (AWS/GitHub/Stripe) are
                // genuine credentials wherever they appear, so they still flag.
                if ctx.file.path_segments.in_test_dir && is_key_block_header(line) {
                    continue;
                }
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
    // Stripe live secret key: sk_live_ / rk_live_ + 24+ base62 (test-mode
    // keys carry a `_test_` segment and are exempt).
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

/// Stripe encodes the key's environment in the prefix: the `_test_` mode
/// segment marks a sandbox credential with no access to production data, which
/// projects routinely commit in examples and tests. Only `_live_` keys reach
/// real data, so only those are flagged — a `sk_test_`/`rk_test_` key is not a
/// secret leak.
fn contains_stripe_key(line: &str) -> bool {
    for prefix in ["sk_live_", "rk_live_"] {
        if let Some(idx) = line.find(prefix) {
            let rest = &line.as_bytes()[idx + prefix.len()..];
            if rest.len() >= 24 && rest[..24].iter().all(|b| b.is_ascii_alphanumeric()) {
                return true;
            }
        }
    }
    false
}

/// True when the value is a Stripe sandbox credential, identified by the
/// `_test_` mode segment that Stripe bakes into the prefix (`sk_test_`,
/// `pk_test_`, `rk_test_`, `whsec_test_`). These keys have no access to
/// production data and are committed as fixtures; they are not a secret leak.
/// Live-mode keys carry no such marker and are not matched here.
fn is_stripe_test_key(value: &str) -> bool {
    const TEST_PREFIXES: &[&str] = &["sk_test_", "pk_test_", "rk_test_", "whsec_test_"];
    TEST_PREFIXES.iter().any(|p| value.starts_with(p))
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

/// True when the line carries a PEM or armored-PGP key block header. The
/// test-directory exemption in [`Check::check`] relies on this to drop fixture
/// key blocks (PGP test vectors, GitHub-App JWT-signing RSA/EC keys) while
/// keeping token-prefix credential shapes (AWS/GitHub/Stripe) flagged. PEM
/// headers reuse [`contains_private_key_header`], so an interpolated PEM frame
/// (`${key}` body) is excluded here exactly as it is everywhere else.
fn is_key_block_header(line: &str) -> bool {
    contains_private_key_header(line)
        || line.contains("-----BEGIN PGP PUBLIC KEY BLOCK-----")
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
    HEADERS.iter().any(|h| match line.find(h) {
        Some(idx) => !pem_body_is_interpolated(line, idx + h.len()),
        None => false,
    })
}

/// A PEM header is only a hardcoded key when the key material between the
/// markers is static. When the header sits inside a template literal whose
/// body is built from a `${...}` substitution, the key is injected at runtime
/// (e.g. `` `-----BEGIN PRIVATE KEY-----\n${key}\n-----END PRIVATE KEY-----` ``)
/// — the header is a format frame, not a credential.
///
/// `header_end` is the byte offset just past the BEGIN marker. We confirm the
/// marker lies inside a backtick template literal and that a `${` substitution
/// follows it within that literal.
fn pem_body_is_interpolated(line: &str, header_end: usize) -> bool {
    let backticks_before = line[..header_end].bytes().filter(|b| *b == b'`').count();
    // An odd number of backticks before the header means it opened a template
    // literal that is still active at the marker.
    if backticks_before % 2 == 0 {
        return false;
    }
    let after = &line[header_end..];
    let literal_end = after.find('`').unwrap_or(after.len());
    after[..literal_end].contains("${")
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

/// Generic placeholder usernames that no real deployment would use as an
/// account name — they only appear in docs, examples, and test fixtures.
const PLACEHOLDER_USERNAMES: &[&str] = &["user", "test", "root", "admin", "postgres", "sa"];

/// Generic placeholder passwords. The literal string `pass`/`password`/etc. is
/// never a real credential; it marks a `proto://user:pass@host` example.
const PLACEHOLDER_PASSWORDS: &[&str] =
    &["pass", "password", "passwd", "test", "root", "admin", "postgres", "mysql"];

/// True when a `username:password` userinfo pair is a well-known generic
/// placeholder (e.g. `user:pass`, `postgres:postgres`) rather than a real
/// credential. The check is decoupled from the hostname: a placeholder pair is
/// a placeholder whether it points at `localhost` or a demo domain like
/// `db.example.com`. A genuine secret (`admin:S3cr3tP@ss`) fails because its
/// password is not in the placeholder set.
fn is_placeholder_credential_pair(username: &str, password: &str) -> bool {
    let username = username.to_ascii_lowercase();
    let password = password.to_ascii_lowercase();
    PLACEHOLDER_USERNAMES.contains(&username.as_str())
        && PLACEHOLDER_PASSWORDS.contains(&password.as_str())
}

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
                // A `${...}` substitution in the userinfo means the credential is
                // built from a runtime template expression, not a hardcoded
                // literal — the same interpolation-is-not-a-literal reasoning
                // `pem_body_is_interpolated` applies to PEM key bodies. The span
                // is scoped to the userinfo (before the `@`), so a `${...}` in the
                // host/port/db after the `@` still flags a literal password.
                let userinfo = &after[..colon + 1 + at];
                if userinfo.contains("${") {
                    return false;
                }
                let username = &after[..colon];
                let password = &rest[..at];
                if is_placeholder_credential_pair(username, password) {
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
    // Stripe sandbox keys (e.g. a `whsec_test_` webhook secret assigned to a
    // `SECRET`-named variable) carry the `_test_` mode segment that marks them
    // as non-production fixtures, so they are not a real credential leak.
    if is_stripe_test_key(&inner) {
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

    /// Run the check as if the file lived under a test directory, so the
    /// `in_test_dir` key-block-fixture exemption is exercised.
    fn run_in_test_dir(source: &str) -> Vec<Diagnostic> {
        use crate::rules::file_ctx::{FileCtx, PathSegments};
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        Check.check(&CheckCtx::for_test_with_file(Path::new("t.ts"), source, &file))
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

    // Regression tests for #5500 — Stripe encodes the environment in the key
    // prefix. A `_test_` mode segment marks a sandbox credential with no access
    // to production data; these are committed as fixtures and are not a leak.
    // The key bodies below are synthetic 24-char fixtures (a repeated marker),
    // not real Stripe keys — they satisfy the rule's 24+ base62 length check
    // without tripping secret scanners.
    const FIXTURE_KEY_BODY: &str = "EXAMPLE0EXAMPLE0EXAMPLE0";

    #[test]
    fn allows_stripe_sk_test_key() {
        // Mirrors stripe/stripe-node test/telemetry.spec.ts — a sandbox secret key.
        let line = format!("const stripe = require('x')('sk_test_{FIXTURE_KEY_BODY}');");
        assert!(run(&line).is_empty());
    }

    #[test]
    fn allows_stripe_rk_test_key() {
        let line = format!("const k = 'rk_test_{FIXTURE_KEY_BODY}';");
        assert!(run(&line).is_empty());
    }

    #[test]
    fn allows_stripe_whsec_test_secret() {
        // Mirrors stripe/stripe-node test/Webhook.spec.ts — a test webhook secret.
        assert!(run("const SECRET = 'whsec_test_secret';").is_empty());
    }

    #[test]
    fn still_flags_stripe_sk_live_key() {
        // Live-mode keys reach production data and remain a real leak.
        let line = format!("const k = 'sk_live_{FIXTURE_KEY_BODY}';");
        assert_eq!(run(&line).len(), 1);
    }

    #[test]
    fn still_flags_stripe_rk_live_key() {
        let line = format!("const k = 'rk_live_{FIXTURE_KEY_BODY}';");
        assert_eq!(run(&line).len(), 1);
    }

    #[test]
    fn flags_private_key_header() {
        assert_eq!(run("const k = '-----BEGIN RSA PRIVATE KEY-----';").len(), 1);
        assert_eq!(run("-----BEGIN OPENSSH PRIVATE KEY-----").len(), 1);
    }

    // Regression tests for #4942 — a PEM template literal whose body is an
    // interpolated variable is a format frame, not a hardcoded key.
    #[test]
    fn allows_pem_template_with_interpolated_body() {
        assert!(
            run(r#"const pem = `-----BEGIN PRIVATE KEY-----\n${privateKeyBase64}\n-----END PRIVATE KEY-----`;"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_rsa_pem_template_with_interpolated_body() {
        assert!(
            run(r#"const pem = `-----BEGIN RSA PRIVATE KEY-----\n${key}\n-----END RSA PRIVATE KEY-----`;"#)
                .is_empty()
        );
    }

    #[test]
    fn still_flags_fully_literal_pem_template() {
        // A template literal with a static key body is still a hardcoded key.
        assert_eq!(
            run(r#"const pem = `-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBg\n-----END PRIVATE KEY-----`;"#)
                .len(),
            1
        );
    }

    #[test]
    fn still_flags_double_quoted_pem_with_dollar_brace_text() {
        // A `${` in a non-template (double-quoted) string is literal text, not a
        // substitution: the PEM header is still hardcoded and must flag.
        assert_eq!(
            run(r#"const pem = "-----BEGIN PRIVATE KEY-----${notInterp}";"#).len(),
            1
        );
    }

    // Regression tests for #5109 — crypto libraries commit armored PGP key
    // blocks as deterministic test vectors. Inside a test directory these are
    // fixture data, not leaked production secrets.
    #[test]
    fn allows_pgp_private_key_block_in_test_dir() {
        assert!(
            run_in_test_dir("'-----BEGIN PGP PRIVATE KEY BLOCK-----',").is_empty()
        );
    }

    #[test]
    fn allows_pgp_public_key_block_in_test_dir() {
        assert!(
            run_in_test_dir("'-----BEGIN PGP PUBLIC KEY BLOCK-----',").is_empty()
        );
    }

    #[test]
    fn still_flags_pgp_private_key_block_in_production_code() {
        // A PGP private key block in non-test code is still a hardcoded secret.
        assert_eq!(run("const k = '-----BEGIN PGP PRIVATE KEY BLOCK-----';").len(), 1);
    }

    // Regression tests for #5504 — GitHub Apps SDKs embed a purpose-generated
    // RSA/EC PEM key as a test fixture to exercise JWT signing against mock
    // servers (octokit/octokit.js test/app.test.ts). Inside a test directory a
    // PEM private key block is fixture data, not a leaked production secret.
    #[test]
    fn allows_rsa_pem_private_key_in_test_dir() {
        assert!(
            run_in_test_dir("const PRIVATE_KEY = `-----BEGIN RSA PRIVATE KEY-----`;").is_empty()
        );
    }

    #[test]
    fn allows_pkcs8_pem_private_key_in_test_dir() {
        assert!(run_in_test_dir("const k = '-----BEGIN PRIVATE KEY-----';").is_empty());
    }

    #[test]
    fn allows_openssh_private_key_in_test_dir() {
        assert!(run_in_test_dir("-----BEGIN OPENSSH PRIVATE KEY-----").is_empty());
    }

    #[test]
    fn allows_ec_private_key_in_test_dir() {
        assert!(run_in_test_dir("const k = '-----BEGIN EC PRIVATE KEY-----';").is_empty());
    }

    #[test]
    fn still_flags_pem_private_key_in_production_code() {
        // A PEM private key in non-test code is a real credential and must flag.
        assert_eq!(run("const k = '-----BEGIN RSA PRIVATE KEY-----';").len(), 1);
    }

    #[test]
    fn still_flags_real_api_secret_in_test_dir() {
        // Token-prefix shapes (AWS access key here) remain leaks wherever they
        // appear — the test-dir exemption only covers PEM/PGP key blocks.
        assert_eq!(
            run_in_test_dir("const k = 'AKIAIOSFODNN7EXAMPLE';").len(),
            1
        );
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

    // Regression tests for #3355 — generic placeholder credentials (user:pass)
    // are placeholders regardless of the hostname, not only on localhost.
    #[test]
    fn allows_generic_placeholder_credentials_on_non_localhost_host() {
        assert!(
            run(r#"pooled: { connectionString: 'postgres://user:pass@db-pool.prisma.io:5432/postgres' },"#)
                .is_empty()
        );
        assert!(
            run(r#"expect(envContent).toContain("DATABASE_URL='postgres://user:pass@db.prisma.io:5432/postgres'")"#)
                .is_empty()
        );
    }

    #[test]
    fn still_flags_high_entropy_credential_on_placeholder_username() {
        // Placeholder username but a real high-entropy password must still flag.
        assert_eq!(
            run(r#"const db = "postgres://admin:S3cr3tPssw0rdxyz@db.example.com:5432/prod";"#).len(),
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

    // Regression tests for #3301 — `mysql` is the official MySQL Docker image's
    // default root password (`MYSQL_ROOT_PASSWORD=mysql`), so `root:mysql` is a
    // well-known dummy credential like `root:root` or `admin:admin`. The host is
    // not part of the placeholder decision, so loopback IPs and named hosts are
    // treated identically.
    #[test]
    fn allows_mysql_docker_root_credential_on_loopback_ip() {
        assert!(
            run(r#"const u = `mysql://root:mysql@127.0.0.1:${port}/drizzle`;"#).is_empty()
        );
    }

    #[test]
    fn allows_mysql_docker_root_credential_on_localhost() {
        assert!(run(r#"const u = "mysql://root:mysql@localhost:3306/drizzle";"#).is_empty());
    }

    #[test]
    fn allows_postgres_placeholder_on_loopback_ip() {
        assert!(
            run(r#"const u = "postgres://postgres:password@127.0.0.1:5432/db";"#).is_empty()
        );
    }

    #[test]
    fn still_flags_real_password_on_non_loopback_host() {
        assert_eq!(
            run(r#"const u = "mysql://admin:S3cretPassword@prod.db.example.com:3306/app";"#).len(),
            1
        );
    }

    #[test]
    fn still_flags_real_password_on_non_loopback_ip() {
        assert_eq!(
            run(r#"const u = "postgres://user:hunter2@10.0.0.5/db";"#).len(),
            1
        );
    }

    // Regression tests for #7641 — a URL whose userinfo is a `${...}` template
    // substitution builds the credential from a runtime expression, not a
    // hardcoded literal (ghostfolio redis.helper.ts).
    #[test]
    fn allows_interpolated_password_in_template_url() {
        assert!(
            run(r#"return `redis://${encodedPassword ? `:${encodedPassword}` : ''}@${host}:${port}/${db}`;"#)
                .is_empty()
        );
    }

    #[test]
    fn allows_interpolated_userinfo_both_parts() {
        assert!(run(r#"const u = `postgres://${user}:${pass}@${host}/db`;"#).is_empty());
    }

    #[test]
    fn still_flags_literal_redis_password() {
        assert_eq!(run("const db = 'redis://admin:hunter2@host:6379';").len(), 1);
    }

    #[test]
    fn still_flags_literal_password_with_interpolation_after_at() {
        // Critical boundary: userinfo `admin:hunter2` is a literal credential;
        // the `${...}` substitutions are all AFTER the `@` (host/port/db),
        // outside the userinfo span, so the `${` guard must not exempt this.
        assert_eq!(
            run(r#"const u = `redis://admin:hunter2@${host}:${port}/${db}`;"#).len(),
            1
        );
    }
}
