//! no-mock-fetch-directly OxcCheck backend — detect direct mocking of HTTP
//! clients in test files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const MOCKED_MODULES: &[&str] = &["axios", "node-fetch"];
const FETCH_GLOBALS: &[&str] = &["global.fetch", "globalThis.fetch"];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::AssignmentExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["fetch"])
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

        match node.kind() {
            AstKind::CallExpression(call) => {
                // Check for `vi.mock('axios')` / `jest.mock('node-fetch')`.
                let Expression::StaticMemberExpression(member) = &call.callee else { return };
                let obj_text = &ctx.source
                    [member.object.span().start as usize..member.object.span().end as usize];
                let prop = member.property.name.as_str();
                let framework = if obj_text == "vi" && prop == "mock" {
                    "vi"
                } else if obj_text == "jest" && prop == "mock" {
                    "jest"
                } else {
                    return;
                };

                for arg in &call.arguments {
                    let oxc_ast::ast::Argument::StringLiteral(s) = arg else { continue };
                    let module = s.value.as_str();
                    if !MOCKED_MODULES.contains(&module) {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Direct mock of `{module}` via `{framework}.mock` — \
                             use MSW to intercept at the network level instead."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::AssignmentExpression(assign) => {
                // Check for `global.fetch = vi.fn()` / `globalThis.fetch = jest.fn()`.
                let left_text = &ctx.source
                    [assign.left.span().start as usize..assign.left.span().end as usize];
                if !FETCH_GLOBALS.contains(&left_text) {
                    return;
                }
                let right_text = &ctx.source
                    [assign.right.span().start as usize..assign.right.span().end as usize];
                if !right_text.contains("vi.fn()") && !right_text.contains("jest.fn()") {
                    return;
                }
                let mock_fn = if right_text.contains("vi.fn()") {
                    "vi.fn()"
                } else {
                    "jest.fn()"
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, assign.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Reassigning `{left_text}` with `{mock_fn}` — \
                         use MSW to intercept at the network level instead."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}
