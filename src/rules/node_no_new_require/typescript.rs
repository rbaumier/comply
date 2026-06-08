//! node-no-new-require backend — flag `new require('...')`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["new_expression"] prefilter = ["require"] => |node, source, ctx, diagnostics|
    let Some(constructor) = node.child_by_field_name("constructor") else { return };
    if constructor.kind() != "identifier" {
        return;
    }
    if constructor.utf8_text(source).unwrap_or("") != "require" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-no-new-require".into(),
        message: "Unexpected `new require(...)`. Separate the require call: `const Mod = require('...'); new Mod()`.".into(),
        severity: Severity::Error,
        span: None,
    });
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
    fn flags_new_require() {
        assert_eq!(run_on("const app = new require('express');").len(), 1);
    }

    #[test]
    fn flags_new_require_start_of_line() {
        assert_eq!(run_on("new require('foo');").len(), 1);
    }

    #[test]
    fn allows_regular_require() {
        assert!(run_on("const express = require('express');").is_empty());
    }

    #[test]
    fn allows_new_other() {
        assert!(run_on("const app = new Express();").is_empty());
    }
}
