//! regex-no-empty-after-reluctant TypeScript / JavaScript / TSX backend.
//!
//! Detects reluctant quantifiers (`*?`, `+?`, `??`) immediately before
//! end-of-pattern, `$`, or `)` — the quantifier always matches the
//! minimum, making its laziness pointless.
//!
//! AST-only detection eliminates FPs from string literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

fn has_useless_reluctant(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let n = bytes.len();
    if n < 2 {
        return false;
    }
    for i in 0..n {
        let q = bytes[i];
        if (q == b'*' || q == b'+' || q == b'?')
            && i + 1 < n
            && bytes[i + 1] == b'?'
            && (i > 0 && bytes[i - 1] != b'\\')
        {
            let after_idx = i + 2;
            if after_idx >= n {
                return true;
            }
            let next = bytes[after_idx];
            if next == b'$' || next == b')' {
                return true;
            }
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_useless_reluctant(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-empty-after-reluctant",
        "Reluctant quantifier before end-of-pattern is useless \u{2014} it always matches the minimum.".into(),
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
    fn flags_reluctant_star_before_dollar() {
        assert_eq!(run_on("const re = /a*?$/;").len(), 1);
    }

    #[test]
    fn flags_reluctant_plus_before_close_paren() {
        assert_eq!(run_on("const re = /(?:a+?)/;").len(), 1);
    }

    #[test]
    fn flags_reluctant_question_before_end() {
        assert_eq!(run_on("const re = /x??/;").len(), 1);
    }

    #[test]
    fn allows_reluctant_followed_by_content() {
        assert!(run_on("const re = /a*?b/;").is_empty());
    }

    #[test]
    fn allows_greedy_before_dollar() {
        assert!(run_on("const re = /a*$/;").is_empty());
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
