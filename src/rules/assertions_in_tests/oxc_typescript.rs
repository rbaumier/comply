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

/// Collect every enclosing function/arrow node for a given node, all
/// the way up to the file root. An `expect(...)` nested in a callback
/// (`await withFreshDb(async (db) => { expect(...); })`) is logically
/// an assertion of every test function it lives inside — not just the
/// innermost callback. Marking only the nearest enclosing function
/// produces false positives on the common resource-bracketing pattern.
fn enclosing_function_ids(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> Vec<oxc_semantic::NodeId> {
    let mut out = Vec::new();
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if matches!(
            ancestor.kind(),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        ) {
            out.push(ancestor.id());
        }
    }
    out
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
        let mut has_assertion: rustc_hash::FxHashSet<oxc_semantic::NodeId> =
            rustc_hash::FxHashSet::default();

        for node in semantic.nodes().iter() {
            let is_assert = match node.kind() {
                AstKind::CallExpression(call) => {
                    let callee_is_expect = match &call.callee {
                        Expression::Identifier(id) => id.name.starts_with("expect"),
                        Expression::StaticMemberExpression(member) => {
                            member.property.name.as_str() == "expect"
                                || member.property.name.starts_with("expect")
                        }
                        _ => false,
                    };
                    if callee_is_expect {
                        true
                    } else {
                        let text = &ctx.source[call.span.start as usize..call.span.end as usize];
                        text.contains("assert")
                            || text.contains(".plan(")
                            || text.contains(".waitFor")
                            || is_testing_library_query(text)
                    }
                }
                AstKind::StaticMemberExpression(member) => {
                    let name = member.property.name.as_str();
                    matches!(name, "should" | "toBe" | "toEqual" | "toMatch" | "toThrow")
                }
                _ => false,
            };

            if is_assert {
                // Mark every enclosing function — the assertion belongs
                // logically to all of them, not just the innermost.
                for func_id in enclosing_function_ids(node, semantic) {
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
                // The test delegates to a caller-supplied callback (`it(name,
                // () => fn())` inside a wrapper whose `fn` param carries the
                // assertions) — the inline body legitimately has none.
                if crate::rules::test_assertion_helpers::delegates_to_outer_param(node, semantic) {
                    continue;
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        // Use a *.test.ts path so the is_test_file gate passes.
        crate::rules::test_helpers::run_oxc_ts_with_path_and_framework(
            src,
            &Check,
            "/tmp/x.test.ts",
            "",
        )
    }

    #[test]
    fn flags_test_without_any_assertion() {
        let src = r#"
            describe("x", () => {
                it("does nothing", () => {
                    const y = 1 + 1;
                });
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_direct_expect_in_it_body() {
        let src = r#"it("works", () => { expect(1).toBe(1); });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_expect_inside_helper_callback() {
        // Regression for rbaumier/comply#29 — expect inside a callback
        // passed to a helper still belongs to the test.
        let src = r#"
            async function withFreshDb(fn) { return fn({}); }
            describe("db", () => {
                it("should do thing", async () => {
                    await withFreshDb(async (db) => {
                        expect(await query(db)).toBe(1);
                    });
                });
            });
        "#;
        assert!(run(src).is_empty());
    }

    // Regression for #260: a test factory delegates its body to a
    // caller-supplied callback parameter.
    #[test]
    fn allows_factory_delegating_to_wrapper_param() {
        let src = r#"
            function txIt(name: string, fn: () => Promise<void>): void {
                it(name, async () => {
                    await fn();
                });
            }
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }
}
