//! zod-transform-requires-pipe oxc backend — flag `.transform()` without `.pipe()`.

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

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression with property `transform`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "transform" {
            return;
        }

        // Check if the parent is a member expression with property `pipe`
        // (i.e. `.transform(fn).pipe(...)`)
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::StaticMemberExpression(parent_member) = parent.kind() {
            if parent_member.property.name.as_str() == "pipe" {
                return;
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.transform()` output is not re-validated — chain `.pipe(z.*)` to assert the output schema.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
