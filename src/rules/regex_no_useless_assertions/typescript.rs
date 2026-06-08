//! regex-no-useless-assertions TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only, so string literals that happen
//! to contain `^` or `$` (URLs like `https://a/b$`, scoped import paths,
//! Tailwind arbitrary values) cannot be mistaken for regex literals.
//! Detection operates on the extracted pattern alone; flags drive the
//! multiline opt-out.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// `$` followed by non-assertion content (not at end of pattern or
/// alternative). In non-multiline mode, such a `$` can never match.
fn has_useless_dollar(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'$' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next != b')' && next != b'|' {
                // Skip `\$` (escaped dollar).
                if i == 0 || bytes[i - 1] != b'\\' {
                    return true;
                }
            }
        }
    }
    false
}

/// `^` preceded by non-assertion content (not at start of pattern or
/// alternative). In non-multiline mode, such a `^` can never match.
fn has_useless_caret(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'^' && i > 0 {
            let prev = bytes[i - 1];
            if prev != b'(' && prev != b'|' && prev != b'[' && prev != b'\\' {
                return true;
            }
        }
    }
    false
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, flags)) = pattern_and_flags(&node, source) else { return };
    if flags.contains('m') {
        return;
    }
    if !has_useless_dollar(pattern) && !has_useless_caret(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-useless-assertions",
        "Assertion is always true or always false and has no effect.".into(),
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
    fn flags_dollar_not_at_end() {
        assert_eq!(run_on(r#"const re = /foo$bar/;"#).len(), 1);
    }

    #[test]
    fn allows_dollar_at_end() {
        assert!(run_on(r#"const re = /foo$/;"#).is_empty());
    }

    #[test]
    fn flags_caret_not_at_start() {
        assert_eq!(run_on(r#"const re = /foo^bar/;"#).len(), 1);
    }

    #[test]
    fn allows_caret_at_start() {
        assert!(run_on(r#"const re = /^foo/;"#).is_empty());
    }

    #[test]
    fn allows_multiline_flag() {
        assert!(run_on(r#"const re = /foo$bar/m;"#).is_empty());
        assert!(run_on(r#"const re = /foo^bar/m;"#).is_empty());
    }

    #[test]
    fn allows_dollar_before_group_close_or_alternation() {
        assert!(run_on(r#"const re = /(foo$)/;"#).is_empty());
        assert!(run_on(r#"const re = /foo$|bar/;"#).is_empty());
    }

    // --- Regression tests for TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_string_with_dollar() {
        let src = r#"const x = "prefix$suffix hover:foo";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_string_with_caret() {
        let src = r#"const u = "https://a/b^c";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_empty() {
        let src = r#"import X from "@scope/pkg";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_negated_char_class_in_lookahead_lookbehind() {
        // Issue #385: [^\w] inside lookahead/lookbehind must not be flagged.
        let src = r#"const pattern = /(?<=[^\w]|^)keyword(?=[^\w]|$)/;"#;
        assert!(run_on(src).is_empty(), "[^\\w] inside lookahead is a char class, not a useless assertion");
    }
}
