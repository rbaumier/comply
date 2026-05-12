use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const STRING_METHODS: &[&str] = &[
    "replace",
    "replaceAll",
    "trim",
    "trimStart",
    "trimEnd",
    "toUpperCase",
    "toLowerCase",
    "substring",
    "slice",
    "concat",
    "padStart",
    "padEnd",
    "normalize",
    "repeat",
];

pub struct Check;

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
            if is_concise_arrow_body(node, semantic) {
                continue;
            }
            let Expression::CallExpression(call) = &expr_stmt.expression else {
                continue;
            };
            let Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            let method = member.property.name.as_str();
            if !STRING_METHODS.contains(&method) {
                continue;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, expr_stmt.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "String method result is ignored \u{2014} strings are immutable, \
                          the return value must be used."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
}

fn is_concise_arrow_body(node: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    let mut ancestors = semantic.nodes().ancestors(node.id());
    let Some(parent) = ancestors.next() else { return false };
    if !matches!(parent.kind(), AstKind::FunctionBody(_)) {
        return false;
    }
    let Some(grandparent) = ancestors.next() else { return false };
    matches!(grandparent.kind(), AstKind::ArrowFunctionExpression(arrow) if arrow.expression)
}
