//! assertions-in-tests OXC backend — test functions must contain at
//! least one assertion.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

/// True for README-snippet test files (`snippets.spec.ts`,
/// `foo.snippets.spec.js`, …). These wrap README code samples in test
/// runners purely to confirm the samples compile and run without throwing;
/// the absence of `expect()` is intentional — the implicit assertion is
/// "this sample does not throw" — so requiring an explicit assertion is a
/// false positive.
fn is_snippet_test_file(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(stem) = name
        .strip_suffix(".spec.ts")
        .or_else(|| name.strip_suffix(".spec.js"))
    else {
        return false;
    };
    stem == "snippets" || stem.ends_with(".snippets")
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

/// True for `reject(new Error(...))` where `reject` is the second parameter
/// of an enclosing `new Promise((resolve, reject) => …)` executor. In
/// promise-returning tests this rejection *is* the assertion: reaching it
/// fails the test with that error, so the test is not assertion-less.
fn is_promise_reject_assertion(
    call: &CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    // First argument must be `new Error(...)` (or any `*Error` constructor).
    let Some(Argument::NewExpression(new_expr)) = call.arguments.first() else {
        return false;
    };
    let Expression::Identifier(ctor) = &new_expr.callee else {
        return false;
    };
    if !ctor.name.ends_with("Error") {
        return false;
    }

    // Callee must be a bare identifier bound to a Promise-executor reject param.
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    let Some(ref_id) = callee.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl = scoping.symbol_declaration(sym_id);
    declaration_is_promise_reject_param(decl, semantic)
}

/// True when `decl` is the second formal parameter of a function passed as the
/// executor to `new Promise(...)`.
fn declaration_is_promise_reject_param(
    decl: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();

    // Find the enclosing function and the binding's span.
    let decl_span = nodes.kind(decl).span();
    let executor_id = std::iter::once(nodes.get_node(decl))
        .chain(nodes.ancestors(decl))
        .find(|anc| {
            matches!(
                anc.kind(),
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
            )
        })
        .map(|anc| anc.id());
    let Some(executor_id) = executor_id else {
        return false;
    };

    // The executor's parent must be `new Promise(...)`.
    let parent_id = nodes.parent_id(executor_id);
    let AstKind::NewExpression(new_expr) = nodes.kind(parent_id) else {
        return false;
    };
    let Expression::Identifier(ctor) = &new_expr.callee else {
        return false;
    };
    if ctor.name.as_str() != "Promise" {
        return false;
    }

    // The binding must be the second formal parameter (the reject slot).
    let params = match nodes.kind(executor_id) {
        AstKind::Function(f) => &f.params,
        AstKind::ArrowFunctionExpression(f) => &f.params,
        _ => return false,
    };
    params.items.get(1).is_some_and(|second| {
        second.span.start <= decl_span.start && decl_span.end <= second.span.end
    })
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
        if !is_test_file(ctx.path) || is_snippet_test_file(ctx.path) {
            return Vec::new();
        }

        // Pass 1: collect node IDs of functions/arrows that contain assertions.
        let mut has_assertion: std::collections::HashSet<oxc_semantic::NodeId> =
            std::collections::HashSet::new();

        for node in semantic.nodes().iter() {
            let is_assert = match node.kind() {
                AstKind::CallExpression(call) => {
                    let callee_is_expect = match &call.callee {
                        // `attest(…)` is the entry point of every
                        // `@arktype/attest` assertion.
                        Expression::Identifier(id) => {
                            id.name.starts_with("expect") || id.name.as_str() == "attest"
                        }
                        Expression::StaticMemberExpression(member) => {
                            member.property.name.as_str() == "expect"
                                || member.property.name.starts_with("expect")
                        }
                        _ => false,
                    };
                    if callee_is_expect || is_promise_reject_assertion(call, semantic) {
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
                // `expr satisfies T` is a compile-time assertion: a test
                // containing one is a type-level test that passes iff the
                // file compiles, so it counts as asserted.
                AstKind::TSSatisfiesExpression(_) => true,
                // A `throw` is a valid assertion mechanism: timing/property/
                // fuzzing tests fail by throwing on a violated condition
                // (`if (after - before > 10) throw new Error(...)`), which the
                // runner reports as a failure — equivalent to `expect(...)`.
                AstKind::ThrowStatement(_) => true,
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        // Use a *.test.ts path so the is_test_file gate passes.
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "/tmp/x.test.ts", &crate::project::ProjectCtx::for_test_with_framework(""), crate::rules::file_ctx::default_static_file_ctx())
    }

    fn run_at(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, path, &crate::project::ProjectCtx::for_test_with_framework(""), crate::rules::file_ctx::default_static_file_ctx())
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

    // Regression for #971 — arktype's `attest(…)` is the entry point of
    // every `@arktype/attest` assertion.
    #[test]
    fn allows_test_with_attest_assertions() {
        let src = r#"it("x", () => { attest(f instanceof Sub).equals(true); attest(f("ff")).snap("y"); });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #971 — `satisfies` marks a type-level test that
    // passes iff the file compiles; no runtime assertion is expected.
    #[test]
    fn allows_type_level_test_with_satisfies() {
        let src = r#"test("assignability", () => { z.string() satisfies z.core.$ZodString; z.number() satisfies z.core.$ZodNumber; });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_test_without_assertion_after_satisfies_support() {
        let src = r#"it("x", () => { setup(); });"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for #1111 — README-snippet test files wrap sample code in
    // test runners purely to confirm the samples compile and run without
    // throwing; the missing `expect()` is intentional, not a smell.
    #[test]
    fn allows_snippets_spec_file() {
        let src = r#"
            describe("snippets", () => {
                it("ReadmeSampleCreateClient_Node", async () => {
                    const client = new ManagementGroupsAPI(new DefaultAzureCredential(), subscriptionId);
                });
            });
        "#;
        assert!(
            run_at(src, "/tmp/snippets.spec.ts").is_empty(),
            "{:?}",
            run_at(src, "/tmp/snippets.spec.ts")
        );
    }

    #[test]
    fn still_flags_non_snippet_spec_without_assertion() {
        // A regular `*.spec.ts` is NOT a snippet file and must still flag.
        let src = r#"it("does nothing", () => { const y = 1 + 1; });"#;
        assert_eq!(run_at(src, "/tmp/feature.spec.ts").len(), 1);
    }

    // Regression for #1396 — a promise-returning test whose assertion
    // mechanism is `reject(new Error(...))` (test fails iff the rejection is
    // reached) must not be flagged as assertion-less.
    #[test]
    fn allows_promise_reject_new_error_as_assertion() {
        let src = r#"
            test("supports cancelling a callback", () =>
              new Promise((done, reject) => {
                const task = requestCallback(() => {
                  reject(new Error("should not be called"));
                });
                cancelCallback(task);
                requestCallback(() => done(undefined));
              }));
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A custom `*Error` constructor counts too — the rejection is still the
    // assertion mechanism.
    #[test]
    fn allows_promise_reject_custom_error_as_assertion() {
        let src = r#"
            test("rejects with custom error", () =>
              new Promise((resolve, reject) => {
                doThing(() => reject(new AssertionError("bad")));
              }));
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // True positive guard: a promise-returning test that only resolves (no
    // `reject(new Error(...))`) and has no assertion must still flag.
    #[test]
    fn still_flags_promise_test_without_reject_error() {
        let src = r#"
            test("resolves only", () =>
              new Promise((resolve) => {
                setup();
                resolve(undefined);
              }));
        "#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // True positive guard: calling a non-Promise-reject identifier `reject`
    // with a `new Error(...)` does not count — only the Promise executor's
    // second parameter is the assertion mechanism.
    #[test]
    fn still_flags_when_reject_is_not_promise_executor_param() {
        let src = r#"
            test("not a promise reject", () => {
              const reject = (e) => e;
              reject(new Error("nope"));
            });
        "#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #2036 — a timing/ReDoS-safety test asserts by throwing
    // when the elapsed time exceeds a budget; the `throw` is the assertion.
    #[test]
    fn allows_test_asserting_via_conditional_throw() {
        let src = r#"
            test("REGEX_VALID_TAG_NAME no ReDoS", () => {
                const before = performance.now();
                REGEX_VALID_TAG_NAME.test("a-----------------------------------!");
                const after = performance.now();
                if (after - before > 10) {
                    throw new Error("REGEX_VALID_TAG_NAME is vulnerable to ReDoS");
                }
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // True positive guard: the word "throw" inside a string literal is not a
    // `throw` statement, so an assertion-less test still fires.
    #[test]
    fn still_flags_test_with_throw_only_in_string() {
        let src = r#"it("x", () => { log("this should throw eventually"); });"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }
}
