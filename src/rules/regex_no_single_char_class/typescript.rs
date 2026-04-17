//! regex-no-single-char-class TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! string literals containing `[x]` (Tailwind arbitrary values, URLs,
//! scoped import paths) cannot false-positive as regex character classes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Scans a regex pattern for single-character `[X]` classes where `X` is
/// not a special byte (`^`, `]`, `\`). Returns the matching snippet
/// (e.g. `"[a]"`) for each hit so the diagnostic can reference it.
fn find_single_char_classes(pattern: &str) -> Vec<String> {
    let mut hits = Vec::new();
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 2 < len {
        if bytes[i] == b'['
            && bytes[i + 1] != b'^'
            && bytes[i + 1] != b'\\'
            && bytes[i + 1] != b']'
            && bytes[i + 2] == b']'
        {
            // Respect backslash escaping of the opening `[`.
            let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
            if backslashes % 2 == 0 {
                hits.push(pattern[i..i + 3].to_string());
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    hits
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "regex" {
        return;
    }
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    for snippet in find_single_char_classes(pattern) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "regex-no-single-char-class",
            format!(
                "Unnecessary single-character class `{}` \u{2014} use the character directly (or escape it).",
                snippet,
            ),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_single_char_class() {
        let diags = run_on(r#"const re = /[a]bc/;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("[a]"));
    }

    #[test]
    fn flags_dot_in_class() {
        let diags = run_on(r#"const re = /[.]foo/;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("[.]"));
    }

    #[test]
    fn allows_multi_char_class() {
        assert!(run_on(r#"const re = /[abc]/;"#).is_empty());
    }

    #[test]
    fn allows_negated_class() {
        assert!(run_on(r#"const re = /[^a]/;"#).is_empty());
    }

    #[test]
    fn allows_escape_in_class() {
        assert!(run_on(r#"const re = /[\d]/;"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[a]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://example.com/[a]/path";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_empty_class_lookalike() {
        let src = r#"import X from "@scope/[a]-pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
