//! regex-optimal-lookaround-quantifier TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! Tailwind arbitrary-value classes, URLs, and scoped import paths inside
//! string literals cannot false-positive as regex lookaround constructs.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Detects quantified expressions at the start/end of lookaround assertions
/// that should only match a constant number of times.
/// Example: `(?=a+)` — the `+` in a lookahead is misleading since the
/// lookahead only checks if `a` is present, not how many times.
fn has_suboptimal_lookaround_quantifier(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 4 < len {
        // Match (?= (?! (?<= (?<!
        if bytes[i] == b'(' && bytes[i + 1] == b'?' {
            let is_lookahead = bytes[i + 2] == b'=' || bytes[i + 2] == b'!';
            let is_lookbehind = bytes[i + 2] == b'<'
                && i + 3 < len
                && (bytes[i + 3] == b'=' || bytes[i + 3] == b'!');

            if is_lookahead || is_lookbehind {
                let content_start = if is_lookbehind { i + 4 } else { i + 3 };
                if let Some(close) = find_close_paren(bytes, i) {
                    let content = &pattern[content_start..close];
                    let cbytes = content.as_bytes();

                    if is_lookahead {
                        // Check end of content for quantifier.
                        let clen = cbytes.len();
                        if clen > 0 && is_quantifier(cbytes[clen - 1]) {
                            return true;
                        }
                    } else {
                        // Lookbehind: check start of content for quantifier on first element.
                        if cbytes.len() > 1 && is_quantifier(cbytes[1]) {
                            return true;
                        }
                    }
                }
            }
        }
        i += 1;
    }
    false
}

fn is_quantifier(b: u8) -> bool {
    b == b'+' || b == b'*'
}

fn find_close_paren(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 1;
    let mut j = start + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j += 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "regex" {
        return;
    }
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_suboptimal_lookaround_quantifier(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-optimal-lookaround-quantifier",
        "Quantifier at the edge of a lookaround is misleading \u{2014} it should match a constant number of times.".into(),
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
    fn flags_quantifier_in_lookahead() {
        assert_eq!(run_on(r#"const re = /(?=a+)/;"#).len(), 1);
    }

    #[test]
    fn allows_no_quantifier_in_lookahead() {
        assert!(run_on(r#"const re = /(?=a)/;"#).is_empty());
    }

    #[test]
    fn flags_star_in_negative_lookahead() {
        assert_eq!(run_on(r#"const re = /(?!a*)/;"#).len(), 1);
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[(?=a+)]:grid";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/(?=b+)/c";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_empty() {
        let src = r#"import "";"#;
        assert!(run_on(src).is_empty());
    }
}
