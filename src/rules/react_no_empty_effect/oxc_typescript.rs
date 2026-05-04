//! react-no-empty-effect OxcCheck backend.
//!
//! Flags `useEffect(() => {})` with an empty callback body.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, FunctionBody, Statement};
use std::sync::Arc;

pub struct Check;

fn body_is_empty(body: &FunctionBody) -> bool {
    body.statements.iter().all(|s| matches!(s, Statement::EmptyStatement(_)))
}

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

        // Callee must be `useEffect`.
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != "useEffect" {
            return;
        }

        // First argument must be a function with an empty block body.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };

        let has_empty_body = match first_arg {
            Argument::ArrowFunctionExpression(arrow) => {
                arrow.expression == false && body_is_empty(&arrow.body)
            }
            Argument::FunctionExpression(func) => {
                func.body.as_ref().is_some_and(|b| body_is_empty(b))
            }
            _ => false,
        };

        if !has_empty_body {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`useEffect` has an empty body \u{2014} remove it or add effect logic.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
