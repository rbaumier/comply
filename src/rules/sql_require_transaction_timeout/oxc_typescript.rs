//! sql-require-transaction-timeout oxc backend — flag `new Pool(...)`,
//! `drizzle(...)`, and `createPool(...)` calls when the file never
//! references `statement_timeout`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn callee_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // File-level guard.
        if ctx.source.contains("statement_timeout") {
            return;
        }

        match node.kind() {
            AstKind::NewExpression(new_expr) => {
                let Some(name) = callee_name(&new_expr.callee) else { return };
                if name != "Pool" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "DB pool config is missing `statement_timeout` — add it to prevent runaway queries.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::CallExpression(call) => {
                let Some(name) = callee_name(&call.callee) else { return };
                if name != "drizzle" && name != "createPool" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "DB pool config is missing `statement_timeout` — add it to prevent runaway queries.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}
