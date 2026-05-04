//! OxcCheck backend — flag `.flat(1)`.

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
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        // Callee must be `*.flat`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "flat" {
            return;
        }
        // Must have exactly one argument that is the number `1`.
        if call.arguments.len() != 1 {
            return;
        }
        let Some(expr) = call.arguments[0].as_expression() else { return };
        let Expression::NumericLiteral(lit) = expr else { return };
        if lit.value != 1.0 {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Passing `1` as the `depth` argument of `.flat()` is unnecessary \u{2014} it is the default.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
