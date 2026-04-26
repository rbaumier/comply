//! regex-no-useless-lazy TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! string literals like `"@scope/pkg"`, URLs, and Tailwind arbitrary-
//! value classes cannot false-positive as regex quantifiers.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Detect useless lazy quantifiers inside a regex pattern.
///
/// Heuristic: flags `{n}?` — an exact count quantifier followed by a
/// lazy `?`. Since `{n}` already matches a fixed length, the `?` has
/// no effect. Range forms like `{n,m}?` are genuinely lazy and are
/// left alone.
fn has_useless_lazy(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'{' {
            // Respect backslash escaping of `{`.
            let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
            if backslashes % 2 != 0 {
                i += 1;
                continue;
            }
            let start = i + 1;
            let mut j = start;
            while j < len && bytes[j].is_ascii_digit() {
                j += 1;
            }
            // `{n}?` — exact count quantifier with useless lazy.
            if j > start && j < len && bytes[j] == b'}' && j + 1 < len && bytes[j + 1] == b'?' {
                return true;
            }
        }
        i += 1;
    }
    false
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_useless_lazy(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-useless-lazy",
        "Useless lazy quantifier \u{2014} the `?` after a fixed quantifier has no effect.".into(),
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
    fn flags_exact_quantifier_lazy() {
        assert_eq!(run_on("const re = /a{3}?/;").len(), 1);
    }

    #[test]
    fn flags_single_exact_lazy() {
        assert_eq!(run_on("const re = /x{1}?b/;").len(), 1);
    }

    #[test]
    fn allows_range_quantifier_lazy() {
        assert!(run_on("const re = /a{1,3}?/;").is_empty());
    }

    #[test]
    fn allows_no_lazy() {
        assert!(run_on("const re = /a{3}/;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/b";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }
}
