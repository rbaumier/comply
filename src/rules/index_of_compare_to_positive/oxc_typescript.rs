//! index-of-compare-to-positive — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["indexOf"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        let op = bin.operator;
        if !matches!(op, BinaryOperator::GreaterThan | BinaryOperator::LessThan) {
            return;
        }

        let right_text =
            &ctx.source[bin.right.span().start as usize..bin.right.span().end as usize];
        let right_text = right_text.trim();

        // `.indexOf(…) > 0` or `.indexOf(…) < 1`
        let is_bad = (op == BinaryOperator::GreaterThan && right_text == "0")
            || (op == BinaryOperator::LessThan && right_text == "1");
        if !is_bad {
            return;
        }

        // Check if left side is a `.indexOf(...)` call.
        let Expression::CallExpression(call) = &bin.left else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "indexOf" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.indexOf(…) > 0` misses index 0 — use `>= 0` or `!== -1`.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
