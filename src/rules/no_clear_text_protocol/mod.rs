//! no-clear-text-protocol — flag clear-text URLs in string literals.
//!
//! ## Why this rule was rewritten
//!
//! The previous implementation was a `TextCheck` that scanned every
//! line for `http://`, `ftp://`, or `telnet://` substrings. Two
//! failure modes:
//!
//! 1. **Comment lines with example URLs** were not skipped — any
//!    `// see "http://example.com"` got flagged because the line
//!    contained both quotes and the protocol prefix.
//! 2. **Bare protocol prefixes used in detection logic** were
//!    flagged: the user reported `if text.contains("http://") || …`
//!    being treated as if `"http://"` were a real insecure URL,
//!    when it's just the search needle.
//!
//! ## How the new rule works
//!
//! Detection is anchored at string-literal nodes in the AST:
//!
//! 1. Walk the tree for string-literal nodes (`string` /
//!    `template_string` for TS; `string_literal` /
//!    `raw_string_literal` for Rust). Comments are never visited.
//! 2. For each string, look at its content:
//!    - Must start with one of the clear-text prefixes.
//!    - Must be **strictly longer** than the prefix itself.
//!      `"http://".len() == 7`, so a 7-char string is just the
//!      needle; a 8+ char string carries an actual host.
//!    - Must not start with a dev-local prefix (`localhost`,
//!      `127.0.0.1`, `0.0.0.0`).
//! 3. Vue: extract `<script>` blocks via `vue_sfc::extract_scripts`,
//!    re-parse with the TS grammar, run the same string-walk logic.
//!
//! ## Language coverage
//!
//! - **TS / JS / TSX**, **Rust**, **Vue** (via `vue_sfc::extract_scripts`).

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;
mod vue;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-clear-text-protocol",
    description: "Clear-text protocol detected — use the encrypted equivalent.",
    remediation: "Replace http:// with https://, ftp:// with sftp://, telnet:// \
                  with ssh://. Clear-text protocols transmit data in the open.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::TreeSitter(Box::new(vue::Check))),
        ],
    }
}

const CLEAR_TEXT_PREFIXES: &[&str] = &["http://", "ftp://", "telnet://"];

const DEV_PREFIXES: &[&str] = &["http://localhost", "http://127.0.0.1", "http://0.0.0.0"];

/// True if `content` is a clear-text URL with an actual host
/// (strictly longer than the bare protocol prefix) and not a dev-
/// local URL. Used by every backend so the heuristic stays in one
/// place.
pub(super) fn is_clear_text_url(content: &str) -> Option<&'static str> {
    let trimmed = trim_string_quotes(content);
    for &prefix in CLEAR_TEXT_PREFIXES {
        if trimmed.starts_with(prefix) && trimmed.len() > prefix.len() {
            if DEV_PREFIXES.iter().any(|d| trimmed.starts_with(d)) {
                return None;
            }
            if XML_NAMESPACE_PREFIXES.iter().any(|ns| trimmed.starts_with(ns)) {
                return None;
            }
            let host = &trimmed[prefix.len()..];
            let host_end = host
                .find(['/', ':', '?', '#'])
                .unwrap_or(host.len());
            let hostname = &host[..host_end];
            if hostname.len() <= 1 {
                return None;
            }
            if DUMMY_HOSTS.contains(&hostname) {
                return None;
            }
            // RFC 2606 / RFC 6761 reserve `.test`, `.invalid`, and
            // `.localhost` as TLDs that never resolve. A URL using one is a
            // synthetic placeholder (e.g. a base for `new URL(relative, base)`
            // parsing), never a real clear-text network endpoint.
            if hostname.ends_with(".test")
                || hostname.ends_with(".invalid")
                || hostname.ends_with(".localhost")
            {
                return None;
            }
            return Some(prefix);
        }
    }
    None
}

const DUMMY_HOSTS: &[&str] = &["example.com", "example.org", "example.net", "test.local"];

// XML namespace URIs are frozen spec identifiers, not network endpoints.
// The browser never makes an HTTP request to these; they are syntax-level
// identifiers (xmlns="http://www.w3.org/2000/svg" etc.).
const XML_NAMESPACE_PREFIXES: &[&str] = &["http://www.w3.org/"];

fn trim_string_quotes(s: &str) -> &str {
    // TS strings: leading `"`, `'`, or backtick.
    if let Some(stripped) = s
        .strip_prefix('"')
        .or_else(|| s.strip_prefix('\''))
        .or_else(|| s.strip_prefix('`'))
    {
        return stripped
            .strip_suffix('"')
            .or_else(|| stripped.strip_suffix('\''))
            .or_else(|| stripped.strip_suffix('`'))
            .unwrap_or(stripped);
    }
    // Rust raw string: `r#"…"#` — strip leading `r#"` and trailing `"#`.
    if let Some(stripped) = s.strip_prefix("r#\"") {
        return stripped.strip_suffix("\"#").unwrap_or(stripped);
    }
    if let Some(stripped) = s.strip_prefix("r\"") {
        return stripped.strip_suffix('"').unwrap_or(stripped);
    }
    s
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn flags_real_http_url() {
        assert_eq!(is_clear_text_url("\"http://api.acme.io\""), Some("http://"));
    }

    #[test]
    fn does_not_flag_bare_prefix() {
        // The user's exact FP — `"http://"` is the needle, not a URL.
        assert!(is_clear_text_url("\"http://\"").is_none());
    }

    #[test]
    fn does_not_flag_localhost() {
        assert!(is_clear_text_url("\"http://localhost:3000\"").is_none());
    }

    #[test]
    fn does_not_flag_loopback() {
        assert!(is_clear_text_url("\"http://127.0.0.1:8080\"").is_none());
    }

    #[test]
    fn flags_ftp_url() {
        assert_eq!(is_clear_text_url("\"ftp://files.acme.io\""), Some("ftp://"));
    }

    #[test]
    fn does_not_flag_https() {
        assert!(is_clear_text_url("\"https://example.com\"").is_none());
    }

    #[test]
    fn handles_rust_raw_string() {
        assert_eq!(
            is_clear_text_url("r#\"http://real-api.io\"#"),
            Some("http://")
        );
    }

    #[test]
    fn does_not_flag_single_char_dummy_host() {
        assert!(is_clear_text_url("\"http://x\"").is_none());
    }

    #[test]
    fn does_not_flag_example_com() {
        assert!(is_clear_text_url("\"http://example.com\"").is_none());
    }

    #[test]
    fn does_not_flag_example_org() {
        assert!(is_clear_text_url("\"http://example.org/path\"").is_none());
    }

    #[test]
    fn does_not_flag_svg_namespace_uri() {
        assert!(is_clear_text_url("\"http://www.w3.org/2000/svg\"").is_none());
    }

    #[test]
    fn does_not_flag_xhtml_namespace_uri() {
        assert!(is_clear_text_url("\"http://www.w3.org/1999/xhtml\"").is_none());
    }

    #[test]
    fn does_not_flag_xml_schema_namespace_uri() {
        assert!(is_clear_text_url("\"http://www.w3.org/2001/XMLSchema\"").is_none());
    }

    #[test]
    fn does_not_flag_dot_test_tld() {
        // .test is reserved by RFC 2606 — Vitest setup files use it as a fake origin.
        assert!(is_clear_text_url("\"http://example.test:3000\"").is_none());
        assert!(is_clear_text_url("\"http://api.example.test\"").is_none());
    }

    // #504 — `.invalid` (RFC 2606) is a synthetic base for relative-URL
    // parsing, never a real network endpoint.
    #[test]
    fn does_not_flag_dot_invalid_tld() {
        assert!(is_clear_text_url("\"http://relative.invalid\"").is_none());
    }

    #[test]
    fn still_flags_real_host() {
        assert_eq!(is_clear_text_url("\"http://api.internal.corp/v1\""), Some("http://"));
    }
}
