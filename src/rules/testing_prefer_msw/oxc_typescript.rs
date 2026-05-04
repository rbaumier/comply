use std::sync::Arc;

use oxc_ast::ast::{AssignmentTarget, Expression};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

const HTTP_CLIENT_MODULES: &[&str] = &["axios", "node-fetch", "cross-fetch"];

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

fn push(diagnostics: &mut Vec<Diagnostic>, ctx: &CheckCtx, span_start: u32) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "Mocking the HTTP client directly is brittle — use MSW to intercept network requests at the handler level.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

/// Check if a call is `<obj>.method(...)` and return the object name.
fn member_call_obj_name<'a>(call: &'a oxc_ast::ast::CallExpression<'a>, method: &str) -> Option<&'a str> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    if member.property.name.as_str() != method {
        return None;
    }
    let Expression::Identifier(obj) = &member.object else {
        return None;
    };
    Some(obj.name.as_str())
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::AssignmentExpression]
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
                // vi.mock('axios') / jest.mock('node-fetch')
                if let Some(obj) = member_call_obj_name(call, "mock")
                    && (obj == "vi" || obj == "jest") {
                        let Some(first_arg) = call.arguments.first() else {
                            return;
                        };
                        let Some(expr) = first_arg.as_expression() else {
                            return;
                        };
                        if let Expression::StringLiteral(lit) = expr
                            && HTTP_CLIENT_MODULES.contains(&lit.value.as_str()) {
                                push(diagnostics, ctx, call.span.start);
                            }
                        return;
                    }

                // jest.spyOn(global, 'fetch') / vi.spyOn(globalThis, 'fetch')
                if let Some(obj) = member_call_obj_name(call, "spyOn")
                    && (obj == "jest" || obj == "vi") {
                        let Some(first_arg) = call.arguments.first() else {
                            return;
                        };
                        let Some(first_expr) = first_arg.as_expression() else {
                            return;
                        };
                        let Expression::Identifier(first_ident) = first_expr else {
                            return;
                        };
                        if !matches!(first_ident.name.as_str(), "global" | "globalThis") {
                            return;
                        }
                        let Some(second_arg) = call.arguments.get(1) else {
                            return;
                        };
                        let Some(second_expr) = second_arg.as_expression() else {
                            return;
                        };
                        if let Expression::StringLiteral(lit) = second_expr
                            && lit.value.as_str() == "fetch" {
                                push(diagnostics, ctx, call.span.start);
                            }
                    }
            }
            // global.fetch = vi.fn()  /  globalThis.fetch = jest.fn()
            AstKind::AssignmentExpression(assign) => {
                let AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
                    return;
                };
                let Expression::Identifier(obj) = &member.object else {
                    return;
                };
                if !matches!(obj.name.as_str(), "global" | "globalThis") {
                    return;
                }
                if member.property.name.as_str() != "fetch" {
                    return;
                }
                let Expression::CallExpression(right_call) = &assign.right else {
                    return;
                };
                if let Some(robj) = member_call_obj_name(right_call, "fn")
                    && (robj == "vi" || robj == "jest") {
                        push(diagnostics, ctx, assign.span.start);
                    }
            }
            _ => {}
        }
    }
}
