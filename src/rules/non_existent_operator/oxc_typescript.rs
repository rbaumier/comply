//! non-existent-operator oxc backend — detect typo operators `=+`, `=-`, `=!`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentOperator, Expression, UnaryOperator};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AssignmentExpression(assign) = node.kind() else { return };

        // Must be a plain `=` assignment (not `+=`, `-=`, etc.)
        if assign.operator != AssignmentOperator::Assign {
            return;
        }

        // RHS must be a unary expression with +, -, or !
        let Expression::UnaryExpression(unary) = &assign.right else { return };
        if !matches!(
            unary.operator,
            UnaryOperator::UnaryPlus | UnaryOperator::UnaryNegation | UnaryOperator::LogicalNot
        ) {
            return;
        }

        // Check adjacency: the `=` and the unary op must be adjacent (no space)
        // to distinguish `x =+1` (typo) from `x = +1` (intentional).
        //
        // In the source, the assignment operator `=` ends where the unary
        // expression starts. We check that the unary expression's span starts
        // immediately after the `=` sign. The `=` sign position: we find it
        // by looking at the byte just before the unary expression span.
        let unary_start = unary.span.start as usize;
        // The `=` is a single byte. Check that the byte immediately before
        // the unary expression is `=` — meaning no space between them.
        if unary_start == 0 {
            return;
        }
        let source_bytes = ctx.source.as_bytes();
        // The byte immediately before the unary expression should be `=`
        // for an adjacent `=+` typo. If there's a space, it's intentional.
        if source_bytes[unary_start - 1] != b'=' {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Typo operator — did you mean `+=`, `-=`, or `!=`?".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
