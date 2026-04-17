//! regex-no-misleading-capturing-group TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! URLs, Tailwind arbitrary-value classes, and import paths inside
//! string literals cannot false-positive as regex literals.

use crate::diagnostic::{Diagnostic, Severity};

/// Detects a capturing group containing alternation (`|`) immediately
/// followed by a quantifier (`+`, `*`, `?`, `{…}`). Such a group is
/// misleading because the capture contents can vary confusingly across
/// repetitions.
fn has_misleading_capture(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        // Opening `(` that is NOT `(?…` (non-capturing / lookaround / named group).
        if bytes[i] == b'(' && i + 1 < len && bytes[i + 1] != b'?' {
            let mut depth = 1;
            let mut j = i + 1;
            let mut has_alternation = false;
            while j < len && depth > 0 {
                match bytes[j] {
                    b'\\' => j += 1,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'|' if depth == 1 => has_alternation = true,
                    _ => {}
                }
                j += 1;
            }
            if depth == 0 && has_alternation && j + 1 < len {
                let next = bytes[j + 1];
                if matches!(next, b'+' | b'*' | b'?' | b'{') {
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
    let Some((pattern, _flags)) = crate::rules::regex_ast::pattern_and_flags(&node, source) else {
        return;
    };
    if !has_misleading_capture(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-misleading-capturing-group",
        "Capturing group with alternation and quantifier is misleading \u{2014} the capture may match different things.".into(),
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
    fn flags_alternation_with_quantifier() {
        assert_eq!(run_on(r#"const re = /(a|b)+/;"#).len(), 1);
    }

    #[test]
    fn allows_capturing_without_quantifier() {
        assert!(run_on(r#"const re = /(a|b)/;"#).is_empty());
    }

    #[test]
    fn flags_alternation_with_star() {
        assert_eq!(run_on(r#"const re = /(foo|bar)*/;"#).len(), 1);
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_string() {
        let src = r#"const x = "has-[>svg]:grid";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_string() {
        let src = r#"const u = "http://a/b/c";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_import_path() {
        let src = r#"import X from "@scope/pkg/sub";"#;
        assert!(run_on(src).is_empty());
    }
}
