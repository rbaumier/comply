//! regex-no-octal TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! URLs, Tailwind arbitrary-value classes, and scoped import paths
//! inside string literals cannot false-positive as regex literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast;

/// Detect octal escapes like `\1`..`\7`, `\00`..`\377` inside a regex
/// pattern. These are ambiguous: they could mean a backreference or an
/// octal character code. Bare `\0` (null) is unambiguous and skipped.
fn has_octal_escape(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'\\' {
            // Count consecutive backslashes.
            let mut c = 0;
            let mut j = i;
            while j < len && bytes[j] == b'\\' {
                c += 1;
                j += 1;
            }
            if c % 2 == 1 {
                // Odd number of backslashes — the last one is an escape.
                let after = i + c;
                if after < len
                    && bytes[after].is_ascii_digit()
                    && bytes[after] != b'8'
                    && bytes[after] != b'9'
                {
                    if bytes[after] == b'0' {
                        // Bare `\0` is fine; `\0` followed by another octal
                        // digit is ambiguous.
                        if after + 1 < len && bytes[after + 1] >= b'0' && bytes[after + 1] <= b'7' {
                            return true;
                        }
                    } else {
                        return true;
                    }
                }
            }
            i += c;
        } else {
            i += 1;
        }
    }
    false
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = regex_ast::pattern_and_flags(&node, source) else { return };
    if !has_octal_escape(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-octal",
        "Octal escape in regex is ambiguous \u{2014} use a named backreference or Unicode escape instead.".into(),
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
    fn flags_octal_escape_in_regex() {
        assert_eq!(run_on(r#"const re = /\1/;"#).len(), 1);
    }

    #[test]
    fn flags_multi_digit_octal() {
        assert_eq!(run_on(r#"const re = /\12/;"#).len(), 1);
    }

    #[test]
    fn allows_null_escape() {
        assert!(run_on(r#"const re = /\0/;"#).is_empty());
    }

    #[test]
    fn flags_octal_after_null() {
        assert_eq!(run_on(r#"const re = /\00/;"#).len(), 1);
    }

    #[test]
    fn allows_no_regex() {
        assert!(run_on("const x = 42;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a/b\\1";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }
}
