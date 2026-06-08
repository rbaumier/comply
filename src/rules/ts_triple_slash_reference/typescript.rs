//! ts-triple-slash-reference backend — flag `/// <reference path="..." />`
//! and `/// <reference types="..." />` directives.
//!
//! Detection: scan top-level comment nodes for the triple-slash pattern.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["comment"] prefilter = ["///"] => |node, source, ctx, diagnostics|
    let text = match std::str::from_utf8(&source[node.byte_range()]) {
        Ok(t) => t,
        Err(_) => return,
    };

    // Must be a single-line comment starting with `/// <reference`
    if !text.starts_with("/// <reference") && !text.starts_with("///<reference") {
        return;
    }

    // Check for path= or types= (not lib= which is generally fine)
    if text.contains("path=") || text.contains("types=") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-triple-slash-reference".into(),
            message: "Triple-slash reference directive is legacy — \
                      use ES `import` instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
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
    fn flags_path_reference() {
        let diags = run_on("/// <reference path=\"foo\" />\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_types_reference() {
        let diags = run_on("/// <reference types=\"node\" />\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_lib_reference() {
        assert!(run_on("/// <reference lib=\"es2015\" />\nconst x = 1;").is_empty());
    }

    #[test]
    fn allows_regular_comments() {
        assert!(run_on("// just a comment\nconst x = 1;").is_empty());
    }
}
