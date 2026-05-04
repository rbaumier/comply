//! no-unsanitized-method OXC backend — flag unsafe HTML-injection method calls
//! whose HTML argument is not a static string literal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Returns the 0-based argument index carrying the HTML payload for the given
/// method name, or `None` if the method is not targeted.
fn html_arg_index(method: &str) -> Option<usize> {
    match method {
        "insertAdjacentHTML" => Some(1),
        "write" | "writeln" | "setHTMLUnsafe" | "createContextualFragment" => Some(0),
        _ => None,
    }
}

/// True when `expr` is a safe, fully-static string expression.
fn is_static_string(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_) => true,
        Expression::TemplateLiteral(tpl) => tpl.expressions.is_empty(),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "insertAdjacentHTML",
            "setHTMLUnsafe",
            "createContextualFragment",
            "writeln",
            "write",
        ])
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

        // Callee must be a member expression.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        let Some(idx) = html_arg_index(method) else {
            return;
        };

        let Some(arg) = call.arguments.get(idx) else {
            return;
        };
        let arg_expr = arg.as_expression();
        let Some(arg_expr) = arg_expr else {
            return;
        };
        if is_static_string(arg_expr) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Calling `{method}` with a non-literal HTML argument is an XSS vector — avoid dynamic HTML injection, or sanitize input first."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
