//! regex-no-duplicate-chars TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! URLs, Tailwind arbitrary-value classes, and import paths inside
//! string literals cannot false-positive as regex char classes.

use crate::diagnostic::{Diagnostic, Severity};

/// Scans a regex pattern for `[...]` character classes containing
/// duplicate single characters (e.g. `[aab]`).
fn has_duplicate_in_char_class(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            // Respect backslash escaping of `[`.
            let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
            if backslashes % 2 != 0 {
                i += 1;
                continue;
            }
            let start = i + 1;
            let content_start = if start < bytes.len() && bytes[start] == b'^' {
                start + 1
            } else {
                start
            };
            let mut j = start;
            // Allow `]` as first char in class.
            if j < bytes.len() && bytes[j] == b']' {
                j += 1;
            }
            while j < bytes.len() && bytes[j] != b']' {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                j += 1;
            }
            if j < bytes.len() {
                let content = &pattern[content_start..j];
                let mut chars: Vec<char> = Vec::new();
                let mut ci = 0;
                let cbytes = content.as_bytes();
                while ci < cbytes.len() {
                    if cbytes[ci] == b'\\' {
                        ci += 2; // skip escape sequences
                        continue;
                    }
                    if ci + 1 < cbytes.len() && cbytes[ci + 1] == b'-' {
                        ci += 3; // skip range like a-z
                        continue;
                    }
                    chars.push(cbytes[ci] as char);
                    ci += 1;
                }
                let len_before = chars.len();
                chars.sort_unstable();
                chars.dedup();
                if chars.len() < len_before {
                    return true;
                }
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    false
}

/// Extract the pattern substring of a tree-sitter `regex` node.
///
/// Prefers the `pattern` field; falls back to manually parsing the
/// node's full text as `/pattern/flags` for grammar versions that
/// don't expose named fields.
fn regex_pattern<'a>(node: &tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    if let Some(pattern_node) = node.child_by_field_name("pattern")
        && let Ok(t) = pattern_node.utf8_text(source)
    {
        return Some(t);
    }
    let full = node.utf8_text(source).ok()?;
    // Strip the surrounding `/.../flags`.
    let inner = full.strip_prefix('/')?;
    let last_slash = inner.rfind('/')?;
    Some(&inner[..last_slash])
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some(pattern) = regex_pattern(&node, source) else { return };
    if !has_duplicate_in_char_class(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-duplicate-chars",
        "Duplicate character in regex character class \u{2014} remove the redundant character.".into(),
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
    fn flags_duplicate_chars() {
        assert_eq!(run_on("const re = /[aab]/;").len(), 1);
    }

    #[test]
    fn flags_duplicate_chars_non_adjacent() {
        assert_eq!(run_on("const re = /[aba]/;").len(), 1);
    }

    #[test]
    fn allows_unique_chars() {
        assert!(run_on("const re = /[abc]/;").is_empty());
    }

    #[test]
    fn allows_ranges() {
        assert!(run_on("const re = /[a-z]/;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_tailwind_data_slot_in_string() {
        let src = r#"const x = "[data-slot=alert]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/b";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_import_path() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_char_class_lookalike_in_comment() {
        let src = "// regex looks like /[aab]/ but this is a comment\nconst x = 1;";
        assert!(run_on(src).is_empty());
    }
}
