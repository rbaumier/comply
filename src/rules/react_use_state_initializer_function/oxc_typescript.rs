//! react-use-state-initializer-function oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const EXPENSIVE_PREFIXES: &[&str] = &[
    "localStorage.",
    "sessionStorage.",
    "JSON.parse(",
    "compute",
    "build",
    "create",
    "parse(",
];

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

        // Callee must be `useState`
        let Expression::Identifier(callee_ident) = &call.callee else {
            return;
        };
        if callee_ident.name.as_str() != "useState" {
            return;
        }

        // Must have at least one argument
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(init_expr) = first_arg.as_expression() else {
            return;
        };

        // Skip safe primitives, arrow functions, and identifiers
        match init_expr {
            Expression::NumericLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::ArrowFunctionExpression(_)
            | Expression::Identifier(_) => return,
            Expression::CallExpression(_) => {}
            _ => return,
        }

        // Check the source text of the init argument for expensive prefixes
        let init_span = init_expr.span();
        let init_start = init_span.start as usize;
        let init_end = init_span.end as usize;
        if init_end > ctx.source.len() {
            return;
        }
        let init_text = &ctx.source[init_start..init_end];

        if EXPENSIVE_PREFIXES.iter().any(|p| init_text.starts_with(p)) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Pass a lazy initializer `() => expr` to `useState` to avoid recomputing on every render.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
