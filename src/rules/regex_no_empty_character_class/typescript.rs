//! regex-no-empty-character-class TypeScript / JavaScript / TSX backend.
//!
//! Flags `[]` (empty character class) inside the tree-sitter `regex`
//! node's pattern. An empty class matches nothing, making the whole
//! regex unmatchable. AST-only detection eliminates FPs from strings
//! containing literal `[]`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

fn has_empty_char_class(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'[' && bytes[i + 1] == b']' {
            return true;
        }
        i += 1;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_empty_char_class(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-empty-character-class",
        "Empty character class `[]` matches nothing \u{2014} add characters or remove it.".into(),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_empty_char_class_in_literal() {
        assert_eq!(run_on("const re = /[]/g;").len(), 1);
    }

    #[test]
    fn allows_non_empty_char_class() {
        assert!(run_on("const re = /[a-z]/;").is_empty());
    }

    #[test]
    fn allows_bracket_in_string() {
        assert!(run_on("const s = \"no regex here\";").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_string() {
        assert!(run_on(r#"const x = "has-[]:grid";"#).is_empty());
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
