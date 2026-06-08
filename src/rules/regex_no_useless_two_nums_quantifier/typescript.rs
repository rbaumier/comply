//! regex-no-useless-two-nums-quantifier TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! URLs, Tailwind arbitrary-value classes, and import paths inside
//! string literals cannot false-positive as regex quantifiers.

use crate::diagnostic::{Diagnostic, Severity};

/// Scan a regex pattern for `{n,n}` quantifiers where both numbers
/// are equal (redundant — equivalent to `{n}`).
///
/// Respects backslash escaping: `\{3,3\}` is a literal, not a quantifier.
fn has_useless_two_nums_quantifier(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'{' {
            // Count preceding backslashes; odd count means the `{` is escaped.
            let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
            if backslashes % 2 != 0 {
                i += 1;
                continue;
            }
            let num1_start = i + 1;
            let mut j = num1_start;
            while j < len && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > num1_start && j < len && bytes[j] == b',' {
                let num1 = &pattern[num1_start..j];
                let num2_start = j + 1;
                let mut k = num2_start;
                while k < len && bytes[k].is_ascii_digit() {
                    k += 1;
                }
                if k > num2_start && k < len && bytes[k] == b'}' {
                    let num2 = &pattern[num2_start..k];
                    if num1 == num2 {
                        return true;
                    }
                }
            }
        }
        i += 1;
    }
    false
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = crate::rules::regex_ast::pattern_and_flags(&node, source) else {
        return;
    };
    if !has_useless_two_nums_quantifier(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-useless-two-nums-quantifier",
        "Redundant quantifier `{n,n}` \u{2014} simplify to `{n}`.".into(),
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
    fn flags_same_min_max() {
        assert_eq!(run_on("const re = /a{3,3}/;").len(), 1);
    }

    #[test]
    fn flags_same_min_max_large() {
        assert_eq!(run_on("const re = /x{10,10}/;").len(), 1);
    }

    #[test]
    fn allows_different_min_max() {
        assert!(run_on("const re = /a{1,3}/;").is_empty());
    }

    #[test]
    fn allows_single_quantifier() {
        assert!(run_on("const re = /a{3}/;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "grid-cols-[minmax(3,3),1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://example.com/a{3,3}/b";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_empty() {
        let src = r#"import X from "@scope/pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
