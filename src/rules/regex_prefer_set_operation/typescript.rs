//! regex-prefer-set-operation TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! string literals containing `(?=...)` text or character class names
//! cannot be mistaken for regex patterns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Detects lookaround-plus-character-class patterns that can be rewritten
/// as `v`-flag set operations (intersection `&&` or subtraction `--`).
///
/// Examples flagged: `(?=\d)\w`, `(?!\w)\s`.
fn has_set_operation_candidate(pattern: &str) -> bool {
    const CANDIDATES: &[&str] = &[
        r"(?=\d)\w",
        r"(?=\w)\d",
        r"(?!\d)\w",
        r"(?!\w)\d",
        r"(?=\s)\w",
        r"(?=\w)\s",
        r"(?!\s)\w",
        r"(?!\w)\s",
    ];
    CANDIDATES.iter().any(|pat| pattern.contains(pat))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "regex" {
        return;
    }
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_set_operation_candidate(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-prefer-set-operation",
        "This lookaround + character pattern can be expressed using a v-flag set operation.".into(),
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
    fn flags_lookahead_with_char_class() {
        assert_eq!(run_on(r"const re = /(?=\d)\w/;").len(), 1);
    }

    #[test]
    fn flags_negative_lookahead_char_class() {
        assert_eq!(run_on(r"const re = /(?!\d)\w/;").len(), 1);
    }

    #[test]
    fn allows_unrelated_lookahead() {
        assert!(run_on(r"const re = /(?=foo)bar/;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_in_string() {
        let src = r#"const x = "has-[(?=\\d)\\w]:hidden";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://example.com/(?=\\d)\\w";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_empty_flag_pattern() {
        let src = r#"import X from "@scope/(?=\\d)\\w";"#;
        assert!(run_on(src).is_empty());
    }
}
