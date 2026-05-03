//! OxcCheck backend for ts-prefer-using-declaration.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

pub struct Check;

const CLEANUP_METHODS: &[&str] = &[
    "close",
    "dispose",
    "destroy",
    "disconnect",
    "release",
    "end",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["using"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else { return };

        let Some(finalizer) = &try_stmt.finalizer else { return };
        if finalizer.body.len() != 1 {
            return;
        }
        let Statement::ExpressionStatement(expr_stmt) = &finalizer.body[0] else { return };
        let Expression::CallExpression(call) = &expr_stmt.expression else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method = member.property.name.as_str();
        if !CLEANUP_METHODS.contains(&method) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, try_stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Use `using` / `await using` instead of try/finally with `.{method}()` (TS 5.2+)."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
