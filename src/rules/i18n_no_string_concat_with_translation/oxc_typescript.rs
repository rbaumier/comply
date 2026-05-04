//! i18n-no-string-concat-with-translation oxc backend — flag binary `+`
//! expressions where one operand is a `t('...')` call.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use std::sync::Arc;

/// True if the expression is a `t('...')` call (callee is `t`, first arg is a string literal).
fn is_t_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else { return false };
    let Expression::Identifier(id) = &call.callee else { return false };
    if id.name.as_str() != "t" {
        return false;
    }
    call.arguments
        .first()
        .and_then(|a| a.as_expression())
        .is_some_and(|e| matches!(e, Expression::StringLiteral(_)))
}

/// Recursively check if any sub-expression is a `t(...)` call.
fn contains_t_call(expr: &Expression) -> bool {
    if is_t_call(expr) {
        return true;
    }
    if let Expression::BinaryExpression(bin) = expr {
        return contains_t_call(&bin.left) || contains_t_call(&bin.right);
    }
    false
}

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
        if bin.operator != BinaryOperator::Addition {
            return;
        }

        if !contains_t_call(&bin.left) && !contains_t_call(&bin.right) {
            return;
        }

        // Skip nested: only flag the outermost `+` expression.
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::BinaryExpression(parent_bin) = parent.kind() {
            if parent_bin.operator == BinaryOperator::Addition {
                return;
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Don't concatenate `t()` results — use interpolation variables in the translation string instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
