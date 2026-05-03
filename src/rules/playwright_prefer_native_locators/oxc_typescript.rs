//! OxcCheck backend for playwright-prefer-native-locators.

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

const ATTRIBUTE_SELECTORS: &[(&str, &str)] = &[
    ("[role=", "getByRole"),
    ("[placeholder=", "getByPlaceholder"),
    ("[alt=", "getByAltText"),
    ("[title=", "getByTitle"),
    ("[data-testid=", "getByTestId"),
];

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
        if member.property.name.as_str() != "locator" {
            return;
        }

        // First argument should be a string.
        let Some(first_arg) = call.arguments.first() else { return };
        let Some(expr) = first_arg.as_expression() else { return };
        let text = match expr {
            Expression::StringLiteral(s) => s.value.as_str(),
            Expression::TemplateLiteral(t) if t.expressions.is_empty() => {
                match t.quasis.first() {
                    Some(q) => q.value.raw.as_str(),
                    None => return,
                }
            }
            _ => return,
        };

        for &(attr, replacement) in ATTRIBUTE_SELECTORS {
            if text.contains(attr) {
                let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Attribute selector `{attr}...]` in `.locator()` — \
                         use `{replacement}()` instead."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                break;
            }
        }
    }
}
