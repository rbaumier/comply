//! regex-no-escape-backspace TypeScript / JavaScript / TSX backend.
//!
//! Flags `[\b]` (backspace escape) inside a character class of the
//! tree-sitter `regex` node's pattern. Inside `[...]`, `\b` means
//! backspace (U+0008), not a word boundary — almost always a mistake.
//!
//! AST-only detection eliminates FPs from arbitrary strings.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

fn has_backspace_in_char_class(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'[' {
            // Scan inside the class for `\b`.
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b']' {
                if bytes[j] == b'\\' && j + 1 < bytes.len() && bytes[j + 1] == b'b' {
                    return true;
                }
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                j += 1;
            }
            i = j;
        }
        i += 1;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_backspace_in_char_class(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-escape-backspace",
        "`[\\b]` matches backspace, not a word boundary \u{2014} use `\\b` outside a character class for word boundaries.".into(),
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
    fn flags_backspace_in_char_class() {
        assert_eq!(run_on(r#"const re = /[\b]/;"#).len(), 1);
    }

    #[test]
    fn allows_word_boundary() {
        assert!(run_on(r#"const re = /\bfoo\b/;"#).is_empty());
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
