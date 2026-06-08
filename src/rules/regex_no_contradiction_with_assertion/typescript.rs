//! regex-no-contradiction-with-assertion TypeScript / JavaScript / TSX
//! backend.
//!
//! Flags patterns where a lookahead/lookbehind assertion contradicts
//! the adjacent element, e.g. `(?=a)b` or `(?!a)a`, which makes the
//! branch unmatchable. AST gating eliminates FPs from string literals
//! that look like regex.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

fn has_contradiction(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 4 < len {
        if bytes[i] == b'('
            && bytes[i + 1] == b'?'
            && bytes[i + 2] == b'='
            && bytes[i + 3] != b')'
            && bytes[i + 3] != b'\\'
            && let Some(close) = find_close_paren(bytes, i)
        {
            let after = close + 1;
            if after < len
                && bytes[after] != b'|'
                && bytes[after] != b')'
                && bytes[after] != b'('
                && bytes[i + 3] != bytes[after]
                && bytes[after].is_ascii_alphanumeric()
                && bytes[i + 3].is_ascii_alphanumeric()
            {
                return true;
            }
        }
        if bytes[i] == b'('
            && bytes[i + 1] == b'?'
            && bytes[i + 2] == b'!'
            && bytes[i + 3] != b')'
            && bytes[i + 3] != b'\\'
            && let Some(close) = find_close_paren(bytes, i)
        {
            let after = close + 1;
            if after < len && bytes[i + 3] == bytes[after] {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn find_close_paren(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 1;
    let mut j = start + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j += 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_contradiction(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-contradiction-with-assertion",
        "Assertion contradicts the pattern around it \u{2014} this branch can never match.".into(),
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
    fn flags_positive_lookahead_contradiction() {
        assert_eq!(run_on(r#"const re = /(?=a)b/;"#).len(), 1);
    }

    #[test]
    fn flags_negative_lookahead_same_char() {
        assert_eq!(run_on(r#"const re = /(?!a)a/;"#).len(), 1);
    }

    #[test]
    fn allows_consistent_lookahead() {
        assert!(run_on(r#"const re = /(?=a)a/;"#).is_empty());
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
