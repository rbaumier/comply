//! elysia-set-status-after-return oxc backend — within a block, flag
//! `set.status = ...` assignments that appear after a `return` statement.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["set.status"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BlockStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::BlockStatement(block) = node.kind() else {
            return;
        };

        let mut returned = false;
        for stmt in &block.body {
            if matches!(stmt, Statement::ReturnStatement(_)) {
                returned = true;
                continue;
            }
            if returned {
                if let Statement::ExpressionStatement(expr_stmt) = stmt {
                    let text = &ctx.source
                        [expr_stmt.span.start as usize..expr_stmt.span.end as usize];
                    let trimmed = text.trim();
                    if trimmed.starts_with("set.status") && trimmed.contains('=') {
                        let (line, column) = byte_offset_to_line_col(
                            ctx.source,
                            expr_stmt.span.start as usize,
                        );
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "`set.status = ...` after `return` has no effect — set the status before returning.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
        }
    }
}
