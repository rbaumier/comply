//! regex-no-super-linear-move TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! URLs, Tailwind arbitrary-value classes, and scoped import paths
//! inside string literals cannot false-positive as regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Detects quantifiers that can cause quadratic runtime. A quantifier
/// followed by the same literal character it matches forces re-scanning
/// on backtrack.
fn has_super_linear_move(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 1 < len {
        // Pattern: X+X or X*X where X is the same literal char.
        if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' {
            let ch = bytes[i];
            if i + 1 < len && (bytes[i + 1] == b'+' || bytes[i + 1] == b'*') {
                let after_quant = i + 2;
                // Skip `?` for lazy quantifier.
                let check_pos = if after_quant < len && bytes[after_quant] == b'?' {
                    after_quant + 1
                } else {
                    after_quant
                };
                if check_pos < len && bytes[check_pos] == ch {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "regex" {
        return;
    }
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_super_linear_move(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-super-linear-move",
        "Quantifier followed by the same element can cause quadratic runtime.".into(),
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
    fn flags_plus_followed_by_same() {
        assert_eq!(run_on(r#"const re = /a+a/;"#).len(), 1);
    }

    #[test]
    fn flags_star_followed_by_same() {
        assert_eq!(run_on(r#"const re = /a*a/;"#).len(), 1);
    }

    #[test]
    fn allows_different_char_after_quantifier() {
        assert!(run_on(r#"const re = /a+b/;"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "grid-cols-[a+a_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a+a.example.com";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@a+a/pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
