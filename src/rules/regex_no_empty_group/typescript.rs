//! regex-no-empty-group TypeScript / JavaScript / TSX backend.
//!
//! Flags `()` (empty capturing group) inside the tree-sitter `regex`
//! node's pattern. AST gating eliminates FPs from arbitrary strings
//! that contain `()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// True when `pattern` contains an empty capturing group `()` outside a
/// character class. Inside `[...]`, `(` and `)` are literal members, not a
/// group, so the scanner tracks an `in_class` flag (set on an unescaped `[`,
/// cleared on an unescaped `]`) and only flags `()` while outside a class.
fn has_empty_group(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    let mut in_class = false;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'[' if !in_class => in_class = true,
            b']' if in_class => in_class = false,
            b'(' if !in_class && bytes.get(i + 1) == Some(&b')') => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !has_empty_group(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-empty-group",
        "Empty capturing group `()` in regex \u{2014} add a pattern or remove it.".into(),
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
    fn flags_empty_group_in_literal() {
        assert_eq!(run_on("const re = /foo()/;").len(), 1);
    }

    #[test]
    fn allows_non_empty_group() {
        assert!(run_on("const re = /foo(bar)/;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_string() {
        assert!(run_on(r#"const x = "has-[>svg]:()";"#).is_empty());
    }

    #[test]
    fn ignores_url_string() {
        assert!(run_on(r#"const u = "http://a/b()";"#).is_empty());
    }

    #[test]
    fn ignores_import_path() {
        assert!(run_on(r#"import X from "@scope/pkg/sub";"#).is_empty());
    }

    // --- Character-class context: `()` inside `[...]` are literal chars,
    // not a capturing group, so they must not be flagged (issue #3773). ---

    #[test]
    fn ignores_parens_in_character_class() {
        assert!(run_on(r#"const re = /[\s"'():;\\/\[\]{}]/;"#).is_empty());
    }

    #[test]
    fn ignores_parens_in_character_class_variant() {
        assert!(run_on(r#"const re = /[;"'\\/\[\](){}]/;"#).is_empty());
    }

    #[test]
    fn ignores_parens_in_character_class_router_shape() {
        assert!(run_on(r#"const re = /[.\\+*[^\]$()]/g;"#).is_empty());
    }

    #[test]
    fn ignores_bare_class_with_parens() {
        assert!(run_on("const re = /[()]/;").is_empty());
    }

    #[test]
    fn flags_empty_group_after_character_class() {
        assert_eq!(run_on("const re = /[abc]()/;").len(), 1);
    }
}
