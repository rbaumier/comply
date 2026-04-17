//! regex-prefer-predefined-assertion TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! lookaround-looking substrings inside string literals, Tailwind classes,
//! URLs, or scoped import paths cannot false-positive.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Lookaround patterns that can be replaced with `\b` or `\B`.
const WORD_BOUNDARY_PATTERNS: &[&str] = &[
    r"(?=\w)(?<=\W)",
    r"(?=\W)(?<=\w)",
    r"(?<=\w)(?=\W)",
    r"(?<=\W)(?=\w)",
];

/// Lookaround patterns replaceable with `^` or `$`.
const ANCHOR_PATTERNS: &[&str] = &["(?<=^)", "(?=$)"];

/// Returns `true` when `pattern` contains any replaceable lookaround.
fn has_replaceable_assertion(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if !pattern.is_char_boundary(i) {
            i += 1;
            continue;
        }
        for pat in WORD_BOUNDARY_PATTERNS.iter().chain(ANCHOR_PATTERNS.iter()) {
            if pattern.get(i..i + pat.len()) == Some(*pat) {
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
    if !has_replaceable_assertion(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-prefer-predefined-assertion",
        "This lookaround can be replaced with a predefined assertion like `\\b`, `^`, or `$`.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_word_boundary_lookaround() {
        assert_eq!(run_on(r"const re = /(?=\w)(?<=\W)/;").len(), 1);
    }

    #[test]
    fn flags_start_anchor_lookaround() {
        assert_eq!(run_on(r"const re = /(?<=^)foo/;").len(), 1);
    }

    #[test]
    fn allows_normal_lookaround() {
        assert!(run_on(r"const re = /(?=foo)/;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_lookaround_lookalike_in_tailwind_string() {
        let src = r#"const x = "group-[(?<=^)]:flex";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_lookaround_lookalike_in_url() {
        let src = r#"const u = "https://example.com/docs#(?=$)";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_lookaround_lookalike_in_scoped_import() {
        let src = r#"import X from "@scope/(?=\\w)(?<=\\W)";"#;
        assert!(run_on(src).is_empty());
    }
}
