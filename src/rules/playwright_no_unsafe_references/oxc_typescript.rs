//! OxcCheck backend for playwright-no-unsafe-references.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

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
        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "evaluate" {
            return;
        }

        // Receiver must be `page`.
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "page" {
            return;
        }

        // Must have exactly one argument and it must be a function.
        if call.arguments.len() != 1 {
            return;
        }
        let Some(arg_expr) = call.arguments[0].as_expression() else { return };
        if !matches!(
            arg_expr,
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
        ) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`page.evaluate()` with a single function \
                      argument — pass captured variables as the \
                      second argument."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
