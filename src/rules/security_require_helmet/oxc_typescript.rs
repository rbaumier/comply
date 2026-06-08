//! security-require-helmet oxc backend — Express app without `helmet()` middleware.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["express"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
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

        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::Identifier(callee) = &call.callee else { return };
        if callee.name.as_str() != "express" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Express app created without `helmet()` middleware — default security headers are missing.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
