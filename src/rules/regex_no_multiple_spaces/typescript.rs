//! regex-no-multiple-spaces TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only — never scans raw text — so
//! string literals containing runs of spaces (URLs, SQL, prose in
//! translation catalogs, etc.) cannot false-positive as regex patterns.
//!
//! Detects two or more consecutive literal space characters inside a
//! regex pattern. Escaped spaces (`\ `) and quantified spaces (`  {2}`,
//! `\s{2,}`) are the intended fix, and quantifiers break the literal
//! run so they're allowed.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast;

/// Returns `true` when `pattern` contains two or more consecutive
/// unescaped literal space characters.
fn has_multiple_spaces(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            // Skip escape sequences so `\ \ ` is not flagged.
            i += 2;
            continue;
        }
        if bytes[i] == b' ' && i + 1 < bytes.len() && bytes[i + 1] == b' ' {
            return true;
        }
        i += 1;
    }
    false
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = regex_ast::pattern_and_flags(&node, source) else {
        return;
    };
    if !has_multiple_spaces(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-multiple-spaces",
        "Multiple consecutive spaces in regex \u{2014} use a quantifier like ` {2}` instead.".into(),
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
    fn flags_double_space_in_literal() {
        assert_eq!(run_on("const re = /foo  bar/;").len(), 1);
    }

    #[test]
    fn flags_triple_space_in_literal() {
        assert_eq!(run_on("const re = /foo   bar/;").len(), 1);
    }

    #[test]
    fn allows_single_space() {
        assert!(run_on("const re = /foo bar/;").is_empty());
    }

    #[test]
    fn allows_quantifier() {
        assert!(run_on("const re = / {2}/;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_string_with_double_space() {
        // Runs of spaces inside a Tailwind class string are not a regex.
        let src = r#"const x = "px-4  py-2  text-sm";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_with_double_space_in_string() {
        let src = r#"const u = "https://example.com/a  b";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        // `/` inside an import path used to be parsed as a regex by the
        // line scanner, producing false positives on any wide spacing
        // that appeared after it on the same line.
        let src = r#"import X from "@tanstack/react-query"; const s = "a  b";"#;
        assert!(run_on(src).is_empty());
    }
}
