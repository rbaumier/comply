//! regex-no-useless-quantifier TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! Tailwind arbitrary-value classes, URLs, and import paths inside
//! string literals cannot false-positive as regex patterns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Detects useless quantifiers inside a regex pattern:
/// - `{1}` — matches exactly once anyway
/// - `{1,1}` — same
/// - Quantifier on an empty group `()+`, `()*`, `()?`, `(){...}`
fn has_useless_quantifier(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Respect escapes: `\{`, `\(` etc. are literals.
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }

        // Detect `{1}` or `{1,1}`.
        if bytes[i] == b'{' {
            let mut j = i + 1;
            let mut num_buf = String::new();
            while j < len && bytes[j].is_ascii_digit() {
                num_buf.push(bytes[j] as char);
                j += 1;
            }
            if j < len && bytes[j] == b'}' && num_buf == "1" {
                return true;
            } else if j < len && bytes[j] == b',' {
                j += 1;
                let mut num_buf2 = String::new();
                while j < len && bytes[j].is_ascii_digit() {
                    num_buf2.push(bytes[j] as char);
                    j += 1;
                }
                if j < len && bytes[j] == b'}' && num_buf == "1" && num_buf2 == "1" {
                    return true;
                }
            }
        }

        // Detect quantifier on empty group: ()+, ()*, ()?, (){...}.
        if bytes[i] == b'(' && i + 2 < len && bytes[i + 1] == b')' {
            let after = bytes[i + 2];
            if after == b'+' || after == b'*' || after == b'?' || after == b'{' {
                return true;
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
    if !has_useless_quantifier(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-useless-quantifier",
        "Useless quantifier \u{2014} it can only match once or matches an empty element.".into(),
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
    fn flags_quantifier_one() {
        assert_eq!(run_on(r#"const re = /a{1}/;"#).len(), 1);
    }

    #[test]
    fn flags_quantifier_one_one() {
        assert_eq!(run_on(r#"const re = /a{1,1}/;"#).len(), 1);
    }

    #[test]
    fn allows_meaningful_quantifier() {
        assert!(run_on(r#"const re = /a{2}/;"#).is_empty());
    }

    #[test]
    fn flags_empty_group_quantified() {
        assert_eq!(run_on(r#"const re = /()+/;"#).len(), 1);
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        // A Tailwind arbitrary value contains `{1}`-looking syntax in JSX/strings;
        // we must never flag string contents.
        let src = r#"const x = "grid-cols-[minmax(0,1fr)]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        // URLs with `()` sequences should not false-positive.
        let src = r#"const u = "https://example.com/path()+/resource";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_empty_group_lookalike() {
        // `()` lookalikes in scoped imports must never trip the rule.
        let src = r#"import X from "@scope/pkg-()+";"#;
        assert!(run_on(src).is_empty());
    }
}
