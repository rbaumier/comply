//! security-require-helmet backend — Express app without `helmet()` middleware.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["express"])
    }

    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["call_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Only check files that import or create Express apps.
        if !ctx.source_contains("express") {
            return;
        }
        // If helmet() is registered anywhere in this file, we're fine.
        if ctx.source_contains("helmet(") {
            return;
        }
        if !diagnostics.is_empty() {
            return;
        }
        let source_bytes = ctx.source.as_bytes();
        let Some(name) = crate::rules::call_expression::call_function_name(node, source_bytes)
        else {
            return;
        };
        if name == "express" {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "Express app created without `helmet()` middleware — default security headers are missing.".into(),
                Severity::Error,
            ));
        }
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_express_without_helmet() {
        let src = "import express from 'express';\nconst app = express();\napp.get('/', handler);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_express_with_helmet() {
        let src = "import express from 'express';\nimport helmet from 'helmet';\nconst app = express();\napp.use(helmet());";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_files_without_express() {
        assert!(run("const x = 1;").is_empty());
    }
}
