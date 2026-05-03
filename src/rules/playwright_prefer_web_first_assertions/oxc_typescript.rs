use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const LOCATOR_METHODS: &[&str] = &[
    "isVisible",
    "isHidden",
    "isEnabled",
    "isDisabled",
    "isChecked",
    "isEditable",
    "textContent",
    "innerText",
    "innerHTML",
    "getAttribute",
    "inputValue",
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
        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        // Check if this is `expect(...)`.
        let Expression::Identifier(callee) = &call.callee else { return };
        if callee.name.as_str() != "expect" {
            return;
        }

        // First argument should be an await expression.
        let Some(first_arg) = call.arguments.first() else { return };
        let Some(arg_expr) = first_arg.as_expression() else { return };
        let Expression::AwaitExpression(await_expr) = arg_expr else { return };

        // The awaited expression should be a call with a locator method.
        let Expression::CallExpression(inner_call) = &await_expr.argument else { return };
        let Expression::StaticMemberExpression(member) = &inner_call.callee else { return };
        let method_name = member.property.name.as_str();
        if !LOCATOR_METHODS.contains(&method_name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use web-first assertions (`toBeVisible`, \
                      `toBeEnabled`, etc.) instead of asserting on \
                      awaited locator methods — they auto-retry."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
