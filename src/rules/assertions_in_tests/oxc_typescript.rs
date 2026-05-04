//! assertions-in-tests OXC backend — test functions must contain at
//! least one assertion.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

const TESTING_LIBRARY_QUERIES: &[&str] = &[
    "getByText", "getByRole", "getByTestId", "getByLabelText",
    "getByPlaceholderText", "getByAltText", "getByTitle", "getByDisplayValue",
    "findByText", "findByRole", "findByTestId", "findByLabelText",
    "findByPlaceholderText", "findByAltText", "findByTitle", "findByDisplayValue",
    "getAllByText", "getAllByRole", "getAllByTestId",
    "findAllByText", "findAllByRole", "findAllByTestId",
];

fn is_testing_library_query(text: &str) -> bool {
    TESTING_LIBRARY_QUERIES.iter().any(|q| text.contains(q))
}

/// Extract the test name from the first string argument.
fn extract_test_name(args: &[Argument]) -> String {
    if let Some(first) = args.first() {
        match first {
            Argument::StringLiteral(s) => return s.value.to_string(),
            Argument::TemplateLiteral(t) if t.expressions.is_empty() => {
                let mut out = String::new();
                for quasi in &t.quasis {
                    out.push_str(quasi.value.raw.as_str());
                }
                return out;
            }
            _ => {}
        }
    }
    "unnamed".to_string()
}

/// Find the nearest enclosing function/arrow for a given node, stopping
/// at function boundaries. Returns the NodeId.
fn nearest_function_id(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::NodeId> {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return Some(ancestor.id());
            }
            _ => {}
        }
    }
    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        // Pass 1: collect node IDs of functions/arrows that contain assertions.
        let mut has_assertion: std::collections::HashSet<oxc_semantic::NodeId> =
            std::collections::HashSet::new();

        for node in semantic.nodes().iter() {
            let is_assert = match node.kind() {
                AstKind::CallExpression(call) => {
                    let text = &ctx.source[call.span.start as usize..call.span.end as usize];
                    text.contains("expect(")
                        || text.contains("expectTypeOf(")
                        || text.contains("assert")
                        || text.contains(".plan(")
                        || text.contains(".waitFor")
                        || is_testing_library_query(text)
                }
                AstKind::StaticMemberExpression(member) => {
                    let name = member.property.name.as_str();
                    matches!(name, "should" | "toBe" | "toEqual" | "toMatch" | "toThrow")
                }
                _ => false,
            };

            if is_assert {
                // Mark the nearest enclosing function as having an assertion.
                if let Some(func_id) = nearest_function_id(node, semantic) {
                    has_assertion.insert(func_id);
                }
            }
        }

        // Pass 2: find test call expressions and check their callbacks.
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };

            // Callee must be bare `it` or `test`.
            let Expression::Identifier(callee) = &call.callee else {
                continue;
            };
            if callee.name.as_str() != "it" && callee.name.as_str() != "test" {
                continue;
            }

            // The callback is the second argument.
            let Some(callback) = call.arguments.get(1) else {
                continue;
            };

            // Get the callback span to check for @ts-expect-error.
            let cb_span = match callback {
                Argument::ArrowFunctionExpression(f) => f.span,
                Argument::FunctionExpression(f) => f.span,
                _ => continue,
            };

            let body_text = &ctx.source[cb_span.start as usize..cb_span.end as usize];
            if body_text.contains("@ts-expect-error") {
                continue;
            }

            // Find the callback's node ID in the semantic tree.
            // The callback is a child of this call expression node.
            let cb_node_id = find_callback_node_id(node, semantic);
            let Some(cb_id) = cb_node_id else {
                continue;
            };

            if !has_assertion.contains(&cb_id) {
                let name = extract_test_name(&call.arguments);
                let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "assertions-in-tests".into(),
                    message: format!(
                        "Test `{name}` has no assertion — add `expect(...)` or similar."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

/// Find the first function/arrow child of a call expression node.
fn find_callback_node_id(
    call_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::NodeId> {
    let nodes = semantic.nodes();
    // Walk through all nodes looking for a function/arrow whose parent is this call.
    for child_node in nodes.iter() {
        let parent_id = nodes.parent_id(child_node.id());
        if parent_id != call_node.id() {
            continue;
        }
        match child_node.kind() {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                return Some(child_node.id());
            }
            _ => {}
        }
    }
    None
}
