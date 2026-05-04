//! react-no-pass-data-to-parent OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

fn is_callback_name(name: &str) -> bool {
    name.starts_with("on")
        && name.len() > 2
        && name.chars().nth(2).is_some_and(|c| c.is_uppercase())
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useEffect"])
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

        // Callee must be `useEffect`
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != "useEffect" {
            return;
        }

        // First argument must be an arrow function
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Argument::ArrowFunctionExpression(arrow) = first_arg else {
            return;
        };

        // Get the single call expression from the body
        let inner_call = if arrow.expression {
            // Expression body: useEffect(() => onUpdate(data), [data])
            if arrow.body.statements.len() != 1 {
                return;
            }
            let Some(Statement::ExpressionStatement(stmt)) = arrow.body.statements.first() else {
                return;
            };
            &stmt.expression
        } else {
            // Block body: useEffect(() => { onSomething(data) }, [data])
            let stmts: Vec<_> = arrow
                .body
                .statements
                .iter()
                .filter(|s| !matches!(s, Statement::EmptyStatement(_)))
                .collect();
            if stmts.len() != 1 {
                return;
            }
            let Some(Statement::ExpressionStatement(stmt)) = stmts.first() else {
                return;
            };
            &stmt.expression
        };

        let Expression::CallExpression(inner) = inner_call else {
            return;
        };

        let Expression::Identifier(func_ident) = &inner.callee else {
            return;
        };
        let func_name = func_ident.name.as_str();

        if !is_callback_name(func_name) {
            return;
        }

        // Skip if it has side effects like fetch or await
        let call_start = inner.span().start as usize;
        let call_end = inner.span().end as usize;
        if call_end <= ctx.source.len() {
            let call_text = &ctx.source[call_start..call_end];
            if call_text.contains("await") || call_text.contains("fetch(") {
                return;
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Effect only calls `{func_name}` to pass data to parent \u{2014} lift state to avoid the extra render cycle."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
