//! OxcCheck backend for react-no-usememo-simple-expression.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement, UnaryOperator};
use std::sync::Arc;

pub struct Check;

fn is_simple_expression(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(_)
        | Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,

        Expression::TemplateLiteral(tpl) => {
            tpl.expressions.iter().all(|e| is_simple_expression(e))
        }

        Expression::BinaryExpression(bin) => {
            is_simple_expression(&bin.left) && is_simple_expression(&bin.right)
        }

        Expression::UnaryExpression(unary) => {
            if matches!(
                unary.operator,
                UnaryOperator::Delete | UnaryOperator::Void
            ) {
                return false;
            }
            is_simple_expression(&unary.argument)
        }

        Expression::StaticMemberExpression(member) => is_simple_expression(&member.object),

        // Computed member (e.g. arr[index]) is NOT simple
        Expression::ComputedMemberExpression(_) => false,

        Expression::ConditionalExpression(cond) => {
            is_simple_expression(&cond.test)
                && is_simple_expression(&cond.consequent)
                && is_simple_expression(&cond.alternate)
        }

        Expression::ParenthesizedExpression(paren) => is_simple_expression(&paren.expression),

        Expression::TSAsExpression(as_expr) => is_simple_expression(&as_expr.expression),
        Expression::TSNonNullExpression(nn) => is_simple_expression(&nn.expression),
        Expression::TSSatisfiesExpression(sat) => is_simple_expression(&sat.expression),

        _ => false,
    }
}

fn is_usememo_call(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => id.name.as_str() == "useMemo",
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                obj.name.as_str() == "React" && member.property.name.as_str() == "useMemo"
            } else {
                false
            }
        }
        _ => false,
    }
}

fn return_expr_from_body<'a>(
    body: &'a oxc_ast::ast::FunctionBody<'a>,
    is_expression: bool,
) -> Option<&'a Expression<'a>> {
    if is_expression {
        body.statements.first().and_then(|s| {
            if let Statement::ExpressionStatement(es) = s {
                Some(&es.expression)
            } else {
                None
            }
        })
    } else {
        if body.statements.len() != 1 {
            return None;
        }
        if let Statement::ReturnStatement(ret) = &body.statements[0] {
            ret.argument.as_ref()
        } else {
            None
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useMemo"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !is_usememo_call(call) {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };

        let ret_expr = match first_arg {
            Argument::ArrowFunctionExpression(arrow) => {
                return_expr_from_body(&arrow.body, arrow.expression)
            }
            Argument::FunctionExpression(func) => {
                func.body.as_ref().and_then(|b| return_expr_from_body(b, false))
            }
            _ => return,
        };
        let Some(ret_expr) = ret_expr else {
            return;
        };
        if !is_simple_expression(ret_expr) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`useMemo` wrapping a trivially cheap expression — memo overhead exceeds the computation.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
