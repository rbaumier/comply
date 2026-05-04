//! react-no-reset-all-state-on-prop-change OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, ArrayExpressionElement, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

fn looks_like_id_prop(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with("id")
        || lower.ends_with("key")
        || lower == "id"
        || lower == "key"
        || lower.contains("userid")
        || lower.contains("itemid")
        || lower.contains("entityid")
}

fn count_setter_calls(body: &oxc_ast::ast::FunctionBody) -> usize {
    let mut count = 0;
    for stmt in &body.statements {
        let oxc_ast::ast::Statement::ExpressionStatement(expr_stmt) = stmt else {
            continue;
        };
        let Expression::CallExpression(call) = &expr_stmt.expression else {
            continue;
        };
        let Expression::Identifier(ident) = &call.callee else {
            continue;
        };
        let name = ident.name.as_str();
        if name.starts_with("set") && name.len() > 3 {
            count += 1;
        }
    }
    count
}

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Must be useEffect(...)
        let Expression::Identifier(callee_ident) = &call.callee else {
            return;
        };
        if callee_ident.name.as_str() != "useEffect" {
            return;
        }

        // First arg: arrow function
        let Some(Argument::ArrowFunctionExpression(arrow)) = call.arguments.first() else {
            return;
        };

        // Body must be a statement block
        let body = &arrow.body;
        if body.statements.is_empty() {
            return;
        }

        let setter_count = count_setter_calls(body);
        if setter_count < 2 {
            return;
        }

        // Second arg: dependency array
        let Some(Argument::ArrayExpression(deps)) = call.arguments.get(1) else {
            return;
        };

        let mut has_id_dep = false;
        for elem in &deps.elements {
            let ArrayExpressionElement::Identifier(ident) = elem else {
                continue;
            };
            if looks_like_id_prop(ident.name.as_str()) {
                has_id_dep = true;
                break;
            }
        }

        if !has_id_dep {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Effect resets {setter_count} states when dependency changes — use `key={{dep}}` on the component instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
