//! dockerfile-label-url-format tree-sitter backend.
//!
//! For every `label_pair`, if the key looks URL-shaped (contains `url`
//! case-insensitively, or matches one of the well-known URL-bearing OCI
//! image labels), require the value to start with `http://` or `https://`
//! after stripping surrounding quotes.

use crate::diagnostic::{Diagnostic, Severity};

const URL_KEYS: &[&str] = &[
    "org.opencontainers.image.url",
    "org.opencontainers.image.source",
    "org.opencontainers.image.documentation",
];

fn key_is_url_shaped(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    if lower.contains("url") {
        return true;
    }
    URL_KEYS.iter().any(|k| lower == *k)
}

fn unquote(text: &str) -> &str {
    let t = text.trim();
    if t.len() >= 2 {
        let bytes = t.as_bytes();
        let first = bytes[0];
        let last = bytes[t.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &t[1..t.len() - 1];
        }
    }
    t
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "label_pair" { return; }
    let key_node = match node.child_by_field_name("key") {
        Some(k) => k,
        None => return,
    };
    let value_node = match node.child_by_field_name("value") {
        Some(v) => v,
        None => return,
    };
    let key_text = match std::str::from_utf8(&source[key_node.byte_range()]) {
        Ok(t) => unquote(t),
        Err(_) => return,
    };
    if !key_is_url_shaped(key_text) { return; }
    let value_text = match std::str::from_utf8(&source[value_node.byte_range()]) {
        Ok(t) => unquote(t),
        Err(_) => return,
    };
    if value_text.is_empty() { return; }
    if value_text.starts_with("http://") || value_text.starts_with("https://") {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "URL-typed LABEL value must start with `http://` or `https://`.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_oci_url_with_non_url_value() {
        let src = "FROM alpine\nLABEL org.opencontainers.image.url=\"example\"\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_custom_url_key_with_non_url_value() {
        let src = "FROM alpine\nLABEL com.example.url=\"foo\"\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_oci_url_with_https() {
        let src = "FROM alpine\nLABEL org.opencontainers.image.url=\"https://example.com\"\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_oci_documentation_with_http() {
        let src = "FROM alpine\nLABEL org.opencontainers.image.documentation=\"http://docs.example.com\"\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_url_keys() {
        let src = "FROM alpine\nLABEL maintainer=\"alice\"\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_empty_url_values() {
        // empty values are flagged by dockerfile-label-not-empty, not here.
        let src = "FROM alpine\nLABEL org.opencontainers.image.url=\"\"\n";
        assert!(run(src).is_empty());
    }
}
