//! prefer-concise-arrow-body — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ArrowFunctionExpression(arrow) = node.kind() else {
            return;
        };

        // Already concise (`() => expr`): nothing to collapse.
        if arrow.expression {
            return;
        }

        // A directive prologue (e.g. "use strict") would be lost on collapse.
        if !arrow.body.directives.is_empty() {
            return;
        }

        // The block must hold exactly one statement: a `return` with a value.
        if arrow.body.statements.len() != 1 {
            return;
        }
        let Statement::ReturnStatement(ret) = &arrow.body.statements[0] else {
            return;
        };
        if ret.argument.is_none() {
            return;
        }

        // Collapsing drops anything that isn't the returned expression, so skip
        // arrows whose source carries a comment rather than suggest a lossy rewrite.
        let arrow_src = &ctx.source[arrow.span.start as usize..arrow.span.end as usize];
        if arrow_src.contains("//") || arrow_src.contains("/*") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "prefer-concise-arrow-body".into(),
            message: "Block-bodied arrow returns a single value; use a concise body.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
