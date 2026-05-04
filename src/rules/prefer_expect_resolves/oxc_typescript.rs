//! prefer-expect-resolves OXC backend — flag `expect(await promise)` calls.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be the identifier `expect`.
        let Expression::Identifier(id) = &call.callee else { return };
        if id.name.as_str() != "expect" {
            return;
        }

        // Must have exactly one argument, and it must be an await expression.
        if call.arguments.len() != 1 {
            return;
        }
        let Argument::AwaitExpression(_) = &call.arguments[0] else { return };

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `await expect(promise).resolves` instead of `expect(await promise)`.".into(),
            severity: Severity::Warning,
            span: Some((call.span.start as usize, (call.span.end - call.span.start) as usize)),
        });
    }
}
