//! regex-no-slow-pattern TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! URLs, Tailwind arbitrary-value classes, and import paths inside
//! string literals cannot false-positive as regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Detects nested quantifiers like `(X+)+`, `(X*)*`, `(X+)*`, `(X*)+`, `(.*)*` etc.
/// These patterns can cause catastrophic backtracking (ReDoS).
fn has_nested_quantifier(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'(' {
            // Find matching closing paren.
            let mut depth = 1;
            let mut j = i + 1;
            let mut inner_has_quantifier = false;
            let mut in_character_class = false;
            while j < len && depth > 0 {
                match bytes[j] {
                    b'\\' => {
                        j += 1; // skip escaped char
                    }
                    b'[' => in_character_class = true,
                    b']' => in_character_class = false,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'+' | b'*' if !in_character_class => inner_has_quantifier = true,
                    _ => {}
                }
                j += 1;
            }
            if depth == 0 && inner_has_quantifier && j + 1 < len {
                let next = bytes[j + 1];
                if next == b'+' || next == b'*' {
                    return true;
                }
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    false
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_nested_quantifier(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-slow-pattern",
        "Nested quantifier detected \u{2014} this pattern can cause catastrophic backtracking (ReDoS).".into(),
        Severity::Warning,
    ));
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_plus_plus() {
        assert_eq!(run_on(r#"const re = /(a+)+/;"#).len(), 1);
    }

    #[test]
    fn flags_star_star() {
        assert_eq!(run_on(r#"const re = /(.*)*$/;"#).len(), 1);
    }

    #[test]
    fn flags_plus_star() {
        assert_eq!(run_on(r#"const re = /(a+)*/;"#).len(), 1);
    }

    #[test]
    fn allows_single_quantifier() {
        assert!(run_on(r#"const re = /(a+)/;"#).is_empty());
    }

    #[test]
    fn allows_non_quantified_group() {
        assert!(run_on(r#"const re = /(abc)/;"#).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "grid-cols-[(a+)+_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/(b+)+";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@scope/(pkg+)+";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_plus_literal_in_character_class() {
        assert!(run_on(r#"const re = /([a+])+/;"#).is_empty());
    }

    #[test]
    fn ignores_star_literal_in_character_class() {
        assert!(run_on(r#"const re = /([*])*/;"#).is_empty());
    }
}
