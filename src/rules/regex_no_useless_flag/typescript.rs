//! regex-no-useless-flag TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only. Detects flags that have no
//! effect on the surrounding pattern:
//!
//! - `i` when the pattern has no literal letters (case-folding is a
//!   no-op).
//! - `m` when the pattern has no `^` or `$` anchors.
//! - `s` when the pattern has no `.` to re-scope.
//!
//! AST-only detection eliminates the TextCheck false-positive class
//! where URLs, import paths, and Tailwind arbitrary-value strings
//! were parsed as `/pattern/flags`.

use crate::diagnostic::{Diagnostic, Severity};

fn has_useless_flag(pattern: &str, flags: &str) -> bool {
    let pbytes = pattern.as_bytes();

    // `i` flag with no unescaped letters outside character classes.
    if flags.contains('i') {
        let mut has_letter = false;
        let mut k = 0;
        while k < pbytes.len() {
            if pbytes[k] == b'\\' {
                k += 2; // skip escape sequences like \d, \w, \s
                continue;
            }
            if pbytes[k] == b'[' {
                // skip character class content — not a literal match
                k += 1;
                while k < pbytes.len() && pbytes[k] != b']' {
                    if pbytes[k] == b'\\' {
                        k += 1;
                    }
                    k += 1;
                }
            }
            if k < pbytes.len() && pbytes[k].is_ascii_alphabetic() {
                has_letter = true;
                break;
            }
            k += 1;
        }
        if !has_letter {
            return true;
        }
    }

    // `m` flag with no ^ or $
    if flags.contains('m') {
        let has_anchor = pbytes.contains(&b'^') || pbytes.contains(&b'$');
        if !has_anchor {
            return true;
        }
    }

    // `s` flag with no unescaped `.`
    if flags.contains('s') {
        let mut k = 0;
        let mut has_dot = false;
        while k < pbytes.len() {
            if pbytes[k] == b'\\' {
                k += 2;
                continue;
            }
            if pbytes[k] == b'.' {
                has_dot = true;
                break;
            }
            k += 1;
        }
        if !has_dot {
            return true;
        }
    }

    false
}

/// Extract pattern + flags from a tree-sitter `regex` node. Prefers
/// named fields; falls back to `/pattern/flags` text parsing when the
/// fields aren't exposed.
fn regex_parts<'a>(node: &tree_sitter::Node<'_>, source: &'a [u8]) -> Option<(&'a str, &'a str)> {
    let pattern = node
        .child_by_field_name("pattern")
        .and_then(|n| n.utf8_text(source).ok());
    let flags = node
        .child_by_field_name("flags")
        .and_then(|n| n.utf8_text(source).ok());

    if let (Some(p), Some(f)) = (pattern, flags) {
        return Some((p, f));
    }
    if let Some(p) = pattern {
        // Pattern present but no flags node — treat flags as empty.
        return Some((p, ""));
    }

    // Grammar fallback: split the raw text on the last `/`.
    let full = node.utf8_text(source).ok()?;
    let inner = full.strip_prefix('/')?;
    let last_slash = inner.rfind('/')?;
    Some((&inner[..last_slash], &inner[last_slash + 1..]))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "regex" {
        return;
    }
    let Some((pattern, flags)) = regex_parts(&node, source) else { return };
    if flags.is_empty() {
        return;
    }
    if !has_useless_flag(pattern, flags) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-useless-flag",
        "Regex flag has no effect on this pattern \u{2014} remove it.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_useless_i_flag() {
        assert_eq!(run_on(r#"const re = /\d+/i;"#).len(), 1);
    }

    #[test]
    fn allows_useful_i_flag() {
        assert!(run_on(r#"const re = /foo/i;"#).is_empty());
    }

    #[test]
    fn flags_useless_m_flag() {
        assert_eq!(run_on(r#"const re = /foo/m;"#).len(), 1);
    }

    #[test]
    fn flags_useless_s_flag() {
        assert_eq!(run_on(r#"const re = /foo/s;"#).len(), 1);
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://localhost:6762/api/v1/diffs/query";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_import_path() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_tailwind_arbitrary_value() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }
}
