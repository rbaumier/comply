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
        if !ctx.source.contains("express") {
            return;
        }
        // If helmet() is registered anywhere in this file, we're fine.
        if ctx.source.contains("helmet(") {
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
