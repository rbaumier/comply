//! prefer-array-from-map oxc backend — flag `[...iter].map(fn)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ArrayExpressionElement, Expression};
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

        // Look for [...iter].map(fn)
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "map" {
            return;
        }

        let Expression::ArrayExpression(arr) = &member.object else { return };

        // Check if array is [...something] (exactly one spread element)
        if arr.elements.len() != 1 {
            return;
        }
        let ArrayExpressionElement::SpreadElement(spread) = &arr.elements[0] else { return };

        // Skip if spreading an array literal
        if matches!(&spread.argument, Expression::ArrayExpression(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `Array.from(iter, mapFn)` instead of `[...iter].map(mapFn)`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
