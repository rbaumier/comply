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

/// True when the test title declares an implicit "must not throw" assertion.
/// A test named `should not throw` / `should not throw when host is void`
/// asserts by *completing without an unhandled exception*: if the body throws,
/// the runner records a failure. The name is explicit documentation of that
/// intent, so a body with no `expect(...)` is intentional, not a smell. The
/// match requires the full phrase (case-insensitive) and is deliberately not
/// broadened to any "throw"/"error" mention.
fn name_declares_no_throw(name: &str) -> bool {
    name.to_ascii_lowercase().contains("should not throw")
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
        // Playwright E2E files import their runner from `@playwright/test` and
        // use Playwright's own assertion/auto-waiting model, not a vitest/jest
        // `expect(...)` — they don't follow this rule's contract.
        if crate::rules::test_assertion_helpers::imports_playwright_test(semantic) {
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
                        // `@arktype/attest` assertion. React render calls
                        // (`render`, `renderToString`, `renderHook`, …) throw
                        // when the component/hook crashes, so they are implicit
                        // "does not throw" assertions. `node:assert` named
                        // exports (`strictEqual`, `deepStrictEqual`, …) throw
                        // `AssertionError` on a failed check, so a direct call
                        // to one is an assertion.
                        Expression::Identifier(id) => {
                            id.name.starts_with("expect")
                                || id.name.as_str() == "attest"
                                || crate::rules::test_assertion_helpers::RENDER_ASSERTION_CALLS
                                    .contains(&id.name.as_str())
                                || crate::rules::test_assertion_helpers::is_node_assert_function(
                                    id.name.as_str(),
                                )
                        }
                        Expression::StaticMemberExpression(member) => {
                            member.property.name.as_str() == "expect"
                                || member.property.name.starts_with("expect")
                        }
                        _ => false,
                    };
                    if callee_is_expect
                        || crate::rules::test_assertion_helpers::is_promise_reject_assertion(
                            call, semantic,
                        )
                        || crate::rules::test_assertion_helpers::is_promise_resolve_call(
                            call, semantic,
                        )
                        || crate::rules::test_assertion_helpers::is_cypress_assertion_call(call)
                    {
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
                    // Chai's `should` BDD style asserts via `value.should.be.equal(x)`
                    // / `arr.should.have.length(3)` / the getter form
                    // `x.should.be.true`. A `.should` member that is chained
                    // further (`.should.<member>`) is the assertion signal; a
                    // `.should` read as a plain value is not.
                    let chai_should = name == "should"
                        && matches!(
                            semantic
                                .nodes()
                                .kind(semantic.nodes().parent_id(node.id())),
                            AstKind::StaticMemberExpression(_)
                                | AstKind::ComputedMemberExpression(_)
                        );
                    matches!(name, "toBe" | "toEqual" | "toMatch" | "toThrow") || chai_should
                }
                // `expr satisfies T` is a compile-time assertion: a test
                // containing one is a type-level test that passes iff the
                // file compiles, so it counts as asserted.
                AstKind::TSSatisfiesExpression(_) => true,
                // A `TSTypeReference` to the `Expect`/`Equal` type-level helpers
                // (`type t = Expect<Equal<typeof x, T>>`) is a compile-time
                // assertion: the file fails to compile if the types differ, so
                // it counts as asserted.
                AstKind::TSTypeReference(type_ref) => matches!(
                    &type_ref.type_name,
                    TSTypeName::IdentifierReference(id)
                        if matches!(id.name.as_str(), "Expect" | "Equal")
                ),
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
                let name = extract_test_name(&call.arguments);
                // A test explicitly named "should not throw" asserts by
                // completing without an unhandled exception — its missing
                // `expect(...)` is intentional.
                if name_declares_no_throw(&name) {
                    continue;
                }
                // The test delegates to a caller-supplied callback (`it(name,
                // () => fn())` inside a wrapper whose `fn` param carries the
                // assertions) — the inline body legitimately has none.
                if crate::rules::test_assertion_helpers::delegates_to_outer_param(node, semantic) {
                    continue;
                }
                // The test delegates to a callback it passes outward whose
                // parameter is supplied by a cross-file tester helper (knex's
                // `.testSql(tester => tester(...))`) — the real assertions live
                // in that helper, in another module.
                if crate::rules::test_assertion_helpers::delegates_to_callback_param(node, semantic)
                {
                    continue;
                }
                // The test's only assertion may live in a same-file helper
                // function (module or describe scope) called from the body.
                let body_span = match callback {
                    Argument::ArrowFunctionExpression(f) => f.body.span,
                    Argument::FunctionExpression(f) => {
                        f.body.as_ref().map_or(f.span, |b| b.span)
                    }
                    _ => cb_span,
                };
                if crate::rules::test_assertion_helpers::body_calls_asserting_local_helper(
                    body_span, semantic,
                ) {
                    continue;
                }
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

    // Regression for #2348: Playwright test files import their runner from
    // `@playwright/test` and use Playwright's own assertion/auto-waiting model,
    // so an action-only or `await expect(locator).toBeVisible()` body is valid.
    #[test]
    fn allows_action_only_playwright_test() {
        let src = r#"
            import { test } from '@playwright/test';
            test('send message', async ({ page }) => {
                await page.goto('/api/auth/signin');
                await page.click('[type="submit"]');
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_vitest_test_without_assertion_no_playwright_import() {
        let src = r#"
            import { test } from 'vitest';
            test('does nothing', () => { const y = 1 + 1; });
        "#;
        assert_eq!(run(src).len(), 1);
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

    // True positive guard: a promise-returning test that never calls its
    // resolve parameter and has no other assertion must still flag — there is
    // no completion path to fail by timeout on.
    #[test]
    fn still_flags_promise_test_that_never_resolves() {
        let src = r#"
            test("resolves only", () =>
              new Promise((resolve) => {
                setup();
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

    // Regression for #1870 — a React Testing Library smoke test whose body is
    // only `render(<App/>)` is a "does not throw" assertion: render throws if
    // the component crashes, so reaching the end means it rendered. Uses a
    // `.tsx` path so the JSX argument parses.
    #[test]
    fn allows_test_with_render_only() {
        let src = r#"it("should not throw error when register with non input ref", () => { render(<App />); });"#;
        assert!(run_at(src, "/tmp/x.test.tsx").is_empty(), "{:?}", run_at(src, "/tmp/x.test.tsx"));
    }

    // Regression for #1870 — SSR smoke test built only on `renderToString`.
    #[test]
    fn allows_test_with_render_to_string_only() {
        let src = r#"it("should render correctly with as with component", () => { renderToString(<Component />); });"#;
        assert!(run_at(src, "/tmp/x.test.tsx").is_empty(), "{:?}", run_at(src, "/tmp/x.test.tsx"));
    }

    // Regression for #1870 — hook smoke test built only on `renderHook` + `act`
    // (no JSX, so a plain `.test.ts` path is fine).
    #[test]
    fn allows_test_with_render_hook_only() {
        let src = r#"it("should reset value", () => { const { result } = renderHook(() => useForm()); result.current.register("test"); act(() => result.current.reset({ test: "test" })); });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // True positive guard: a test with no assertion and no render call still
    // fires — the render carve-out must not swallow genuinely empty tests.
    #[test]
    fn still_flags_test_without_render_or_assertion() {
        let src = r#"it("x", () => { const x = 1; });"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #1856 — a promise-returning test whose only completion
    // path calls the resolve parameter (`done`) of `new Promise(done => …)`
    // asserts by timeout: if `done` is never reached the test fails. No
    // `reject(new Error(...))` is involved.
    #[test]
    fn allows_promise_resolve_done_as_assertion() {
        let src = r#"
            describe("requestCallback basics", () => {
              test("queue a task", () =>
                new Promise(done => {
                  requestCallback(() => {
                    done(undefined);
                  });
                }));
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // True positive guard: a bare `done(undefined)` where `done` is NOT the
    // first parameter of a `new Promise(...)` executor does not count.
    #[test]
    fn still_flags_when_done_is_not_promise_executor_param() {
        let src = r#"
            test("not a promise resolve", (done) => {
              done(undefined);
            });
        "#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #1677 — the test's only assertion lives in a helper
    // defined in the same `describe` scope and called from the body.
    #[test]
    fn allows_test_calling_describe_scope_helper_with_assertion() {
        let src = r#"
            describe("Cache-Control header", () => {
                describe("is not set", () => {
                    const shouldNotSetCacheControlHeader = (response) => {
                        expect(response.headers.get("cache-control")).toBeUndefined();
                    };
                    it("is not set when disabled", async () => {
                        const response = await makePlugin();
                        shouldNotSetCacheControlHeader(response);
                    });
                });
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #1617 — same root cause for a module-scope helper.
    #[test]
    fn allows_test_calling_module_scope_helper_with_assertion() {
        let src = r#"
            function assertOk(response) {
                expect(response.status).toBe(200);
            }
            it("returns ok", async () => {
                const response = await fetchThing();
                assertOk(response);
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // True positive guard for #1677: a helper with no assertion does not
    // launder an empty test.
    #[test]
    fn still_flags_test_calling_helper_without_assertion() {
        let src = r#"
            const setupThing = (x) => { const y = x + 1; };
            it("does nothing", () => {
                setupThing(1);
            });
        "#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // True positive guard for #1677: a test that asserts nothing and calls
    // nothing still fires.
    #[test]
    fn still_flags_test_with_no_assertion_and_no_call() {
        let src = r#"it("x", () => { const y = 1 + 1; });"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #2339 — knex's `.testSql(tester => tester(...))` pattern:
    // the callback's `tester` parameter is supplied by a cross-file helper
    // (`testSqlTester` in logger.js) that performs the real `expect(...)`. The
    // test body only invokes that supplied parameter, so its assertions live in
    // another module.
    #[test]
    fn allows_test_delegating_to_cross_file_tester_callback_param() {
        let src = r#"
            it("should handle simple inserts", async function () {
                await knex("accounts")
                    .insert({ first_name: "Test" }, "id")
                    .testSql(function (tester) {
                        tester("mysql", "insert into `accounts` ...", [], 1);
                        tester("pg", "insert into \"accounts\" ...", [], [{ id: 1 }]);
                    });
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Negative-space guard for #2339: a plain non-asserting call in the body
    // (the callee is not a callback parameter supplied from outside) must still
    // fire — the carve-out must not silence genuinely assertion-less tests.
    #[test]
    fn still_flags_test_with_plain_body_call() {
        let src = r#"it("x", () => { doThing(); });"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #1380 — Cypress assertions chain `.should(...)` onto a `cy`
    // command instead of calling `expect(...)`. A test ending in such a chain
    // does assert and must not be flagged.
    #[test]
    fn allows_cypress_should_contain_assertion() {
        let src = r#"it("underlines text", () => { cy.get("button:first").click(); cy.get(".tiptap").find("u").should("contain", "Example Text"); });"#;
        assert!(
            run_at(src, "/tmp/index.spec.js").is_empty(),
            "{:?}",
            run_at(src, "/tmp/index.spec.js")
        );
    }

    #[test]
    fn allows_cypress_should_not_exist_assertion() {
        let src = r#"it("toggles", () => { cy.get(".tiptap").find("u").should("not.exist"); });"#;
        assert!(
            run_at(src, "/tmp/index.spec.js").is_empty(),
            "{:?}",
            run_at(src, "/tmp/index.spec.js")
        );
    }

    // Negative-space guard for #1380: a Cypress test with only commands and no
    // `.should()`/`.and()` assertion still passes silently — keep flagging it.
    #[test]
    fn still_flags_cypress_test_without_should() {
        let src = r#"it("clicks", () => { cy.get("button").click(); });"#;
        assert_eq!(
            run_at(src, "/tmp/index.spec.js").len(),
            1,
            "{:?}",
            run_at(src, "/tmp/index.spec.js")
        );
    }

    // A bare `.should(...)` on a non-`cy` receiver is not a Cypress assertion —
    // the chain root must be the `cy` identifier.
    #[test]
    fn still_flags_should_on_non_cy_receiver() {
        let src = r#"it("x", () => { wrapper.find("u").should("contain", "x"); });"#;
        assert_eq!(
            run_at(src, "/tmp/index.spec.js").len(),
            1,
            "{:?}",
            run_at(src, "/tmp/index.spec.js")
        );
    }

    // Regression for #1215 — node:assert named exports (`strictEqual`,
    // `deepStrictEqual`, …) destructured from `node:assert` or a wrapper like
    // `@effect/vitest/utils` are assertion functions: they throw `AssertionError`
    // on mismatch. A test whose only assertion is such a call must not be flagged.
    #[test]
    fn allows_test_with_node_assert_named_exports() {
        let src = r#"
            import { deepStrictEqual, strictEqual } from "@effect/vitest/utils";
            it("joins path without trailing slash on base", () => {
                strictEqual(request.url, "https://api.example.com/v1/users");
            });
            it("parses URL instances", () => {
                deepStrictEqual(actual, expected);
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // True positive guard for #1215: a call to an ordinary, non-assertion
    // function with a similar arity must still flag the test as assertion-less —
    // only the known node:assert names count.
    #[test]
    fn still_flags_test_with_ordinary_call_only() {
        let src = r#"it("x", () => { foo(a, b); });"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Negative-space guard for #1215: a test with no assertion of any kind (no
    // expect/assert/strictEqual/…) is still flagged.
    #[test]
    fn still_flags_test_with_no_assertion_at_all() {
        let src = r#"it("does nothing", () => { const y = compute(1, 2); return y; });"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #1306 — a compile-time type assertion via the
    // `Expect<Equal<typeof x, T>>` type-alias idiom is the assertion: the file
    // fails to compile if the types differ, so the test has no runtime expect
    // by design.
    #[test]
    fn allows_test_with_expect_equal_type_alias() {
        let src = r#"
            it("infers correct type", () => {
                type t = Expect<Equal<typeof result, { id: number }>>;
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #1306 — `expectTypeOf(...)` is a type-only assertion helper.
    #[test]
    fn allows_test_with_expect_type_of_value_form() {
        let src = r#"it("expectTypeOf form", () => { expectTypeOf(result).toEqualTypeOf<{ id: number }>(); });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Negative-space guard for #1306: a test with no assertion at all — neither a
    // runtime expect nor a compile-time type assertion — must still fire.
    #[test]
    fn still_flags_test_without_type_assertion() {
        let src = r#"it("x", () => { type t = { id: number }; const y = 1 + 1; });"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #2307 — chai's `should` BDD style adds a `.should` property
    // to every object, so assertions read `value.should.be.equal(x)` /
    // `arr.should.have.length(N)` without any `expect(...)`/`assert(...)` call.
    #[test]
    fn allows_chai_should_call_form_assertion() {
        let src = r#"it("inserts", () => { loadedUsers1.length.should.be.equal(10); loadedUsers2.length.should.be.equal(3); });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_chai_should_have_length_assertion() {
        let src = r#"it("has length", () => { const arr = getItems(); arr.should.have.length(3); });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #2307 — getter form `result.should.be.true` has no call at
    // all; the `.should` member is the assertion signal.
    #[test]
    fn allows_chai_should_getter_form_assertion() {
        let src = r#"it("is true", () => { const result = check(); result.should.be.true; });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Negative-space guard for #2307: a property literally named `should` that is
    // not the head of a chai assertion chain (read as a value) is not an
    // assertion, so an otherwise empty test still fires.
    #[test]
    fn still_flags_test_with_should_property_read() {
        let src = r#"it("x", () => { const n = config.should; log(n); });"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Regression for #2353 — a test explicitly named "should not throw" asserts
    // by completing without an unhandled exception; a plain expression/assignment
    // body with no `expect(...)` is intentional, not assertion-less.
    #[test]
    fn allows_no_throw_named_test_with_plain_body() {
        let src = r#"it("should not throw", () => { response().status = 403 });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_no_throw_named_test_with_suffix() {
        let src = r#"it("should not throw when host is void", () => { const req = request(); req.host = undefined });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Negative-space guard for #2353: a test whose name does NOT contain
    // "should not throw" and whose body has no assertion must still fire — the
    // name heuristic is the only exemption signal and must stay tight.
    #[test]
    fn still_flags_assertionless_test_without_no_throw_name() {
        let src = r#"it("returns the status", () => { response().status = 403 });"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }
}
