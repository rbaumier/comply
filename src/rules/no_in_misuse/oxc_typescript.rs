//! no-in-misuse oxc backend — flag `x in arr` where `arr` looks like an array.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BinaryOperator;
use oxc_span::GetSpan;
use std::sync::Arc;

const ARRAY_HINTS: &[&str] = &[
    "arr", "list", "items", "elements", "values", "entries", "rows", "results",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        if bin.operator != BinaryOperator::In {
            return;
        }

        // Skip `for ... in` — the parent is a ForInStatement.
        let parent = semantic.nodes().parent_node(node.id());
        if matches!(parent.kind(), AstKind::ForInStatement(_)) {
            return;
        }

        let rhs_start = bin.right.span().start as usize;
        let rhs_end = bin.right.span().end as usize;
        let rhs_text = &ctx.source[rhs_start..rhs_end];

        let lower = rhs_text.to_ascii_lowercase();
        let looks_like_array = rhs_text.starts_with('[')
            || ARRAY_HINTS.iter().any(|hint| lower.contains(hint));

        if !looks_like_array {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`in` operator checks object keys, not array values — use `.includes()` instead.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
