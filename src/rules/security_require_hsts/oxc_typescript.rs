//! security-require-hsts oxc backend —
//! Express app without HSTS header (no helmet, no Strict-Transport-Security).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
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
        if !ctx.source_contains("express") {
            return;
        }
        // helmet() installs HSTS by default, so that's accepted.
        if ctx.source_contains("helmet(") {
            return;
        }
        // Explicit HSTS header is also accepted.
        if ctx.source_contains("Strict-Transport-Security")
            || ctx.source_contains("strict-transport-security")
        {
            return;
        }
        // Only emit once per file.
        if !diagnostics.is_empty() {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };
        let name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if name != "express" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Express app has no HSTS header — install `helmet()` or set `Strict-Transport-Security`.".into(),
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
    fn flags_express_without_hsts() {
        let src = "import express from 'express';\nconst app = express();\napp.get('/', h);";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_express_with_helmet() {
        let src = "import express from 'express';\nimport helmet from 'helmet';\nconst app = express();\napp.use(helmet());";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_express_with_explicit_hsts_header() {
        let src = "import express from 'express';\nconst app = express();\napp.use((req,res,next)=>{res.setHeader('Strict-Transport-Security','max-age=31536000');next();});";
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_non_express_files() {
        assert!(run("const x = 1;").is_empty());
    }
}
