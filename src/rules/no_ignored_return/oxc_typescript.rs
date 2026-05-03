//! no-ignored-return OXC backend — flag standalone calls to pure methods
//! whose return value is ignored.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;

pub struct Check;

const PURE_METHODS: &[&str] = &[
    "map",
    "filter",
    "slice",
    "concat",
    "trim",
    "replace",
    "toUpperCase",
    "toLowerCase",
    "split",
    "join",
];

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::ExpressionStatement(expr_stmt) = node.kind() else {
                continue;
            };
            let Expression::CallExpression(call) = &expr_stmt.expression else {
                continue;
            };
            let Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            let method_name = member.property.name.as_str();
            if !PURE_METHODS.contains(&method_name) {
                continue;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, expr_stmt.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Return value of `.{}` is ignored — the call has no side effect.",
                    method_name
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
