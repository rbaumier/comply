//! regex-no-trivially-nested-quantifier TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! parenthesised strings like `"(?:a+)?"` inside code, Tailwind classes,
//! or URLs cannot false-positive. Detection operates on the extracted
//! `pattern` of the regex literal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Detects trivially nested quantifiers that can be merged, returning
/// byte offsets (within the pattern) where each nested group starts.
/// Example: `(?:a{2}){3}` can be `a{6}`, or `(?:a+)?` can be `a*`.
fn find_trivially_nested_quantifiers(pattern: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Look for `(?:` non-capturing group.
        if bytes[i] == b'(' && i + 2 < len && bytes[i + 1] == b'?' && bytes[i + 2] == b':' {
            let group_start = i;
            let content_start = i + 3;
            let mut depth = 1;
            let mut j = content_start;

            while j < len && depth > 0 {
                match bytes[j] {
                    b'\\' => j += 1,
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            // j is now one past the closing paren.
            let close = j - 1; // position of ')'
            if depth == 0 {
                let content = &pattern[content_start..close];
                // Check: single element with quantifier, e.g. `a+`, `a*`, `a?`, `a{2}`
                let has_inner_quantifier = is_single_quantified_element(content);
                // Check outer quantifier.
                if has_inner_quantifier && close + 1 < len {
                    let next = bytes[close + 1];
                    if next == b'+' || next == b'*' || next == b'?' || next == b'{' {
                        hits.push(group_start);
                    }
                }
            }
        }
        i += 1;
    }
    hits
}

/// Returns true if the content is a single element followed by a quantifier.
/// E.g. `a+`, `a*`, `.?`, `a{2,3}`, `\d+`.
fn is_single_quantified_element(content: &str) -> bool {
    let bytes = content.as_bytes();
    let clen = bytes.len();
    if clen < 2 {
        return false;
    }

    // Determine element length.
    let elem_len;
    if bytes[0] == b'\\' {
        // Escaped char like `\d`, `\w`, `\s`.
        elem_len = 2;
    } else if bytes[0] == b'[' {
        // Character class.
        if let Some(close) = find_char_class_close(bytes, 0) {
            elem_len = close + 1;
        } else {
            return false;
        }
    } else if bytes[0] == b'.' || bytes[0].is_ascii_alphanumeric() {
        elem_len = 1;
    } else {
        return false;
    }

    if elem_len >= clen {
        return false;
    }

    // Rest must be a quantifier.
    let rest = bytes[elem_len];
    match rest {
        b'+' | b'*' | b'?' => {
            elem_len + 1 == clen || (elem_len + 2 == clen && bytes[elem_len + 1] == b'?')
        }
        b'{' => bytes[elem_len..].contains(&b'}'),
        _ => false,
    }
}

fn find_char_class_close(bytes: &[u8], start: usize) -> Option<usize> {
    let mut j = start + 1;
    if j < bytes.len() && bytes[j] == b'^' {
        j += 1;
    }
    if j < bytes.len() && bytes[j] == b']' {
        j += 1; // literal ] at start of class
    }
    while j < bytes.len() {
        if bytes[j] == b'\\' {
            j += 2;
            continue;
        }
        if bytes[j] == b']' {
            return Some(j);
        }
        j += 1;
    }
    None
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if find_trivially_nested_quantifiers(pattern).is_empty() {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-trivially-nested-quantifier",
        "Trivially nested quantifiers can be merged into a single quantifier.".into(),
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
    fn flags_nested_plus_optional() {
        assert_eq!(run_on(r#"const re = /(?:a+)?/;"#).len(), 1);
    }

    #[test]
    fn allows_multi_element_group() {
        assert!(run_on(r#"const re = /(?:ab)+/;"#).is_empty());
    }

    #[test]
    fn flags_nested_star_plus() {
        assert_eq!(run_on(r#"const re = /(?:a*)+/;"#).len(), 1);
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_quantifier_lookalike_in_tailwind_string() {
        let src = r#"const x = "has-[(?:a+)?]:block";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_quantifier_lookalike_in_url() {
        let src = r#"const u = "http://ex/(?:a+)?";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_empty_scoped_import_path() {
        let src = r#"import X from "@scope/(?:a+)?";"#;
        assert!(run_on(src).is_empty());
    }
}
