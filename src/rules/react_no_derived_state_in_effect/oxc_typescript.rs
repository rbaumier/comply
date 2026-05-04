//! react-no-derived-state-in-effect oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useEffect"])
    }

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

        // Check callee is `useEffect`.
        let Expression::Identifier(callee_ident) = &call.callee else {
            return;
        };
        if callee_ident.name.as_str() != "useEffect" {
            return;
        }

        // First argument must be an arrow function.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let oxc_ast::ast::Argument::ArrowFunctionExpression(arrow) = first_arg else {
            return;
        };

        // Body must have exactly one statement.
        let body = &arrow.body.statements;
        if body.len() != 1 {
            return;
        }
        let Statement::ExpressionStatement(expr_stmt) = &body[0] else {
            return;
        };
        let Expression::CallExpression(inner_call) = &expr_stmt.expression else {
            return;
        };

        // Check for side-effect patterns: await, fetch(), subscribe(), addEventListener()
        let inner_start = inner_call.span.start as usize;
        let inner_end = inner_call.span.end as usize;
        if inner_end <= ctx.source.len() {
            let call_text = &ctx.source[inner_start..inner_end];
            if call_text.contains("await")
                || call_text.contains("fetch(")
                || call_text.contains("subscribe(")
                || call_text.contains("addEventListener(")
            {
                return;
            }
        }

        // Check that the inner call is a setter (starts with "set").
        let inner_name = match &inner_call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if !inner_name.starts_with("set") {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Derived state in `useEffect` is an anti-pattern. Compute the value during render instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
