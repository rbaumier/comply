//! ts-prefer-optional-chain oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, LogicalOperator};
use std::sync::Arc;

pub struct Check;

/// True if `expr` is `a.b` / `a.b.c` member chain rooted at the same
/// identifier as `root_name`.
fn extends_root_chain(expr: &Expression, root_name: &str) -> bool {
    match expr {
        Expression::Identifier(id) => id.name.as_str() == root_name,
        Expression::StaticMemberExpression(member) => {
            extends_root_chain(&member.object, root_name)
        }
        Expression::ComputedMemberExpression(member) => {
            extends_root_chain(&member.object, root_name)
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::LogicalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::LogicalExpression(logical) = node.kind() else {
            return;
        };
        if logical.operator != LogicalOperator::And {
            return;
        }
        // Pattern: <a> && <a.b> — left is an identifier, right is a
        // member access on the same identifier.
        let Expression::Identifier(left_id) = &logical.left else {
            return;
        };
        let Expression::StaticMemberExpression(right_member) = &logical.right else {
            return;
        };
        if !extends_root_chain(&logical.right, left_id.name.as_str()) {
            return;
        }
        let _ = right_member; // suppress unused-binding warning
        let (line, column) = byte_offset_to_line_col(ctx.source, logical.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`a && a.b` is the classic optional-chain pattern — write `a?.b` \
                      instead. Same short-circuit, less repetition."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
