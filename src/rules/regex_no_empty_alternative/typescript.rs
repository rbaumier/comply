//! regex-no-empty-alternative TypeScript / JavaScript / TSX backend.
//!
//! Detects empty alternatives in a regex: a leading, trailing, or
//! consecutive `|` that makes one branch match the empty string. AST
//! gating eliminates FPs from string literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

fn pattern_has_empty_alternative(pattern: &str) -> bool {
    pattern.starts_with('|') || pattern.ends_with('|') || pattern.contains("||")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !pattern_has_empty_alternative(pattern) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-empty-alternative",
        "Empty alternative in regex \u{2014} remove leading, trailing, or consecutive `|`.".into(),
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
    fn flags_leading_pipe() {
        assert_eq!(run_on("const re = /|foo/;").len(), 1);
    }

    #[test]
    fn flags_trailing_pipe() {
        assert_eq!(run_on("const re = /foo|/;").len(), 1);
    }

    #[test]
    fn flags_consecutive_pipes() {
        assert_eq!(run_on("const re = /foo||bar/;").len(), 1);
    }

    #[test]
    fn allows_valid_alternatives() {
        assert!(run_on("const re = /foo|bar/;").is_empty());
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
