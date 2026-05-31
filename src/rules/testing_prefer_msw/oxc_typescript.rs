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

/// True when `call` is `vi.spyOn(global|globalThis, "fetch")` / `jest.spyOn(...)`.
fn is_global_fetch_spy_on(call: &oxc_ast::ast::CallExpression) -> bool {
    let Some(obj) = member_call_obj_name(call, "spyOn") else { return false };
    if !matches!(obj, "vi" | "jest") {
        return false;
    }
    let Some(Expression::Identifier(first)) = call.arguments.first().and_then(|a| a.as_expression())
    else {
        return false;
    };
    if !matches!(first.name.as_str(), "global" | "globalThis") {
        return false;
    }
    matches!(
        call.arguments.get(1).and_then(|a| a.as_expression()),
        Some(Expression::StringLiteral(lit)) if lit.value.as_str() == "fetch"
    )
}

/// True when the file is a unit test of a `fetch` wrapper: it spies on the global
/// `fetch` and rejects it with a transport-layer error (`mockRejectedValue` /
/// `mockRejectedValueOnce`). MSW intercepts at the HTTP-response layer and cannot
/// reproduce a rejected `fetch` promise carrying a caller-supplied error instance
/// (`TypeError`, `DOMException`), so such a file legitimately mocks `fetch` directly
/// for every case — splitting it across MSW and `vi.spyOn` would be incoherent.
fn is_fetch_wrapper_unit_test(semantic: &oxc_semantic::Semantic) -> bool {
    let nodes = semantic.nodes();
    for node in nodes.iter() {
        let AstKind::CallExpression(call) = node.kind() else { continue };
        if !is_global_fetch_spy_on(call) {
            continue;
        }
        if let AstKind::StaticMemberExpression(member) = nodes.parent_node(node.id()).kind()
            && matches!(
                member.property.name.as_str(),
                "mockRejectedValue" | "mockRejectedValueOnce"
            )
        {
            return true;
        }
    }
    false
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }

        // A fetch-wrapper unit test must mock `fetch` directly to exercise
        // transport-layer rejections MSW cannot reproduce; exempt the file.
        if is_fetch_wrapper_unit_test(semantic) {
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
                if is_global_fetch_spy_on(call) {
                    push(diagnostics, ctx, call.span.start);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts_with_path;

    fn run(src: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_path(src, &Check, "fetch-wrapper.test.ts")
    }

    #[test]
    fn flags_spy_on_global_fetch() {
        let src = r#"vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response());"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_vi_mock_axios() {
        assert_eq!(run(r#"vi.mock("axios");"#).len(), 1);
    }

    #[test]
    fn skips_fetch_wrapper_unit_test_with_transport_rejection() {
        // Regression for issues #518 / #564: a unit test of a fetch wrapper that
        // rejects the global fetch with a transport-layer error MSW cannot
        // reproduce — the whole file legitimately mocks fetch directly.
        let src = r#"
            it("network TypeError", async () => {
                const networkError = new TypeError("Failed to fetch");
                vi.spyOn(globalThis, "fetch").mockRejectedValue(networkError);
            });
            it("ok response", async () => {
                vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response("{}"));
            });
        "#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn still_flags_response_only_mocking_without_transport_rejection() {
        // A test file that only mocks HTTP responses (no transport rejection)
        // should still prefer MSW.
        let src = r#"
            it("ok response", async () => {
                vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response("{}"));
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
