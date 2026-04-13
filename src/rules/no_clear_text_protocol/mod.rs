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

mod rust;
mod typescript;
mod vue;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-clear-text-protocol",
    description: "Clear-text protocol detected — use the encrypted equivalent.",
    remediation: "Replace http:// with https://, ftp:// with sftp://, telnet:// \
                  with ssh://. Clear-text protocols transmit data in the open.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::TreeSitter(Box::new(vue::Check))),
        ],
    }
}

const CLEAR_TEXT_PREFIXES: &[&str] = &["http://", "ftp://", "telnet://"];

const DEV_PREFIXES: &[&str] = &[
    "http://localhost",
    "http://127.0.0.1",
    "http://0.0.0.0",
];

/// True if `content` is a clear-text URL with an actual host
/// (strictly longer than the bare protocol prefix) and not a dev-
/// local URL. Used by every backend so the heuristic stays in one
/// place.
pub(super) fn is_clear_text_url(content: &str) -> Option<&'static str> {
    // Strip surrounding quote characters that the AST node text
    // includes — both `"…"` / `'…'` / `` `…` `` for TS and `"…"` /
    // `r#"…"#` for Rust. Cheap heuristic: trim the well-known
    // wrappers from both ends.
    let trimmed = trim_string_quotes(content);
    for &prefix in CLEAR_TEXT_PREFIXES {
        if trimmed.starts_with(prefix) && trimmed.len() > prefix.len() {
            if DEV_PREFIXES.iter().any(|d| trimmed.starts_with(d)) {
                return None;
            }
            return Some(prefix);
        }
    }
    None
}

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
        assert_eq!(is_clear_text_url("\"http://example.com\""), Some("http://"));
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
        assert_eq!(
            is_clear_text_url("\"ftp://files.example.com\""),
            Some("ftp://")
        );
    }

    #[test]
    fn does_not_flag_https() {
        assert!(is_clear_text_url("\"https://example.com\"").is_none());
    }

    #[test]
    fn handles_rust_raw_string() {
        assert_eq!(
            is_clear_text_url("r#\"http://example.com\"#"),
            Some("http://")
        );
    }
}
