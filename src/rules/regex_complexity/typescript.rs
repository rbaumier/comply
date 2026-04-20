//! regex-complexity TypeScript / JavaScript / TSX backend.
//!
//! Scores regex complexity by counting special constructs in the
//! tree-sitter `regex` node's pattern, and flags patterns scoring above
//! a threshold. AST-only detection eliminates false positives from
//! URLs, import paths, and Tailwind arbitrary-value strings that
//! looked like `/pattern/flags` to the prior line-scanning check.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

fn complexity_score(pattern: &str) -> usize {
    let mut score = 0;
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                if i + 1 < bytes.len() && matches!(bytes[i + 1], b'b' | b'B') {
                    score += 1;
                }
                i += 2;
                continue;
            }
            b'*' | b'+' | b'?' | b'{' | b'|' | b'(' | b'[' | b'^' | b'$' => score += 1,
            _ => {}
        }
        i += 1;
    }
    score
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    let threshold = ctx.config.threshold("regex-complexity", "max");
    let score = complexity_score(pattern);
    if score <= threshold {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-complexity",
        format!(
            "Regex complexity score is {score} (threshold: {threshold}) \u{2014} consider breaking it into smaller patterns."
        ),
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
    fn flags_complex_regex() {
        let complex = r#"const re = /^(a+|b*|c?)(d{2,3})(e|f|g|h)(i+|j*)(k?|l{1})(m|n|o)(p+|q*)(r?)/;"#;
        assert_eq!(run_on(complex).len(), 1);
    }

    #[test]
    fn allows_simple_regex() {
        assert!(run_on(r#"const re = /^hello$/;"#).is_empty());
    }

    #[test]
    fn allows_moderate_regex() {
        assert!(run_on(r#"const re = /\d{3}-\d{4}/;"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_string() {
        assert!(run_on(r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#).is_empty());
    }

    #[test]
    fn ignores_url_string() {
        assert!(run_on(r#"const u = "http://a/b/c";"#).is_empty());
    }

    #[test]
    fn ignores_import_path() {
        assert!(run_on(r#"import X from "@scope/pkg/sub";"#).is_empty());
    }
}
