//! elysia-require-method-chaining OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const ELYSIA_METHODS: &[&str] = &[
    "state",
    "get",
    "post",
    "put",
    "patch",
    "delete",
    "use",
    "guard",
    "onBeforeHandle",
    "onAfterHandle",
    "onError",
    "onRequest",
    "derive",
    "resolve",
    "decorate",
    "model",
    "listen",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop_name = member.property.name.as_str();
        if !ELYSIA_METHODS.contains(&prop_name) {
            return;
        }

        // In a proper chain, the object is a call_expression. If it's an
        // identifier, the chain has been broken.
        let Expression::Identifier(obj_id) = &member.object else { return };

        // Ensure the call is an expression statement (not part of a chain).
        let parent = semantic.nodes().parent_node(node.id());
        if !matches!(parent.kind(), AstKind::ExpressionStatement(_)) {
            return;
        }

        let obj_name = obj_id.name.as_str();
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{obj_name}.{prop_name}(...)` breaks the Elysia method chain \u{2014} type inference is lost. Chain methods on `new Elysia()` directly."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
