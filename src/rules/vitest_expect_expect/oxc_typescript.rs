//! vitest-expect-expect oxc backend — reuses the assertions_in_tests body-text scan.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__")
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

/// True when `text` contains a call to a function whose identifier starts
/// with `expect` or `assert`. The identifier must be word-boundary-anchored
/// on the left so we don't match `inspect(` or `reassert(`. A TypeScript
/// type-argument list (`<...>`) between the identifier and the call's `(`
/// is allowed — `expectTypeOf<X>()` is a runtime call. A member chain is
/// allowed too — `assert.deepStrictEqual(...)` / `expect.soft(...)` are calls
/// on vitest's `assert`/`expect` objects.
fn has_assertion_prefixed_call(text: &str) -> bool {
    let bytes = text.as_bytes();
    for prefix in ["expect", "assert"] {
        let plen = prefix.len();
        let mut from = 0usize;
        while let Some(rel) = text[from..].find(prefix) {
            let i = from + rel;
            // Word boundary on left.
            let prev_ok = i == 0
                || !(bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'$');
            if prev_ok {
                // Skip past prefix and any identifier chars; first
                // non-ident byte must be `(` or `<` (TS generic call).
                let mut j = i + plen;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric()
                        || bytes[j] == b'_'
                        || bytes[j] == b'$')
                {
                    j += 1;
                }
                if bytes.get(j) == Some(&b'(') {
                    return true;
                }
                if bytes.get(j) == Some(&b'.') {
                    // Member chain: `assert.deepStrictEqual(` / `expect.soft(`.
                    // Skip `.identifier` segments; a `(` after one is a call.
                    while bytes.get(j) == Some(&b'.') {
                        j += 1;
                        while j < bytes.len()
                            && (bytes[j].is_ascii_alphanumeric()
                                || bytes[j] == b'_'
                                || bytes[j] == b'$')
                        {
                            j += 1;
                        }
                        if bytes.get(j) == Some(&b'(') {
                            return true;
                        }
                    }
                }
                if bytes.get(j) == Some(&b'<') {
                    // expectTypeOf<X>() / assertType<Y>() — look for `>(`
                    // in a bounded window after the `<`. Track brace depth
                    // so that `;` inside object types like `{ a: string; b: number }`
                    // does not prematurely terminate the scan.
                    let scan_end = (j + 256).min(bytes.len());
                    let mut k = j + 1;
                    let mut brace_depth: i32 = 0;
                    while k + 1 < scan_end {
                        match bytes[k] {
                            b'{' => brace_depth += 1,
                            b'}' => brace_depth -= 1,
                            // `;` only ends the statement when outside braces.
                            b';' if brace_depth == 0 => break,
                            b'>' if brace_depth == 0 && bytes[k + 1] == b'(' => {
                                return true;
                            }
                            _ => {}
                        }
                        k += 1;
                    }
                }
            }
            from = i + plen;
        }
    }
    false
}

/// True when `text` contains a Testing Library query that throws on a missing
/// element — `getBy*`, `getAllBy*`, `findBy*`, `findAllBy*` (e.g.
/// `screen.getByText('x')`, `await screen.findByRole('button')`). These act as
/// implicit assertions: the test fails if the queried element is absent, so a
/// body relying solely on them is a real test, not a silent pass.
///
/// `queryBy*` / `queryAllBy*` are deliberately excluded — they return `null`
/// instead of throwing, so they are not assertions on their own.
fn has_throwing_query_call(text: &str) -> bool {
    let bytes = text.as_bytes();
    for prefix in ["getBy", "getAllBy", "findBy", "findAllBy"] {
        let plen = prefix.len();
        let mut from = 0usize;
        while let Some(rel) = text[from..].find(prefix) {
            let i = from + rel;
            // Word boundary on the left so `myGetByText` doesn't match.
            let prev_ok = i == 0
                || !(bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'$');
            if prev_ok {
                // The query is the start of an identifier (`getByText`,
                // `findByRole`, …); skip the rest of it and require a `(`.
                let mut j = i + plen;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric()
                        || bytes[j] == b'_'
                        || bytes[j] == b'$')
                {
                    j += 1;
                }
                if bytes.get(j) == Some(&b'(') {
                    return true;
                }
            }
            from = i + plen;
        }
    }
    false
}

/// True when a Cypress assertion call — a `.should(...)`/`.and(...)` member
/// call rooted at the global `cy` identifier — lies within `body_span`. Cypress
/// tests assert by chaining `should`/`and` onto a `cy` command rather than
/// calling `expect(...)`, so a body containing one is not assertion-less.
fn body_contains_cypress_assertion(
    semantic: &oxc_semantic::Semantic<'_>,
    body_span: oxc_span::Span,
) -> bool {
    semantic.nodes().iter().any(|n| {
        let AstKind::CallExpression(call) = n.kind() else {
            return false;
        };
        call.span.start >= body_span.start
            && call.span.end <= body_span.end
            && crate::rules::test_assertion_helpers::is_cypress_assertion_call(call)
    })
}

/// True when a TS `satisfies` expression lies within `body_span`.
fn body_contains_satisfies(
    semantic: &oxc_semantic::Semantic<'_>,
    body_span: oxc_span::Span,
) -> bool {
    semantic.nodes().iter().any(|n| {
        if let AstKind::TSSatisfiesExpression(sat) = n.kind() {
            sat.span.start >= body_span.start && sat.span.end <= body_span.end
        } else {
            false
        }
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["test(", "it("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) || is_snippet_test_file(ctx.path) {
            return;
        }
        // Playwright E2E files import their runner from `@playwright/test` and
        // use Playwright's own assertion/auto-waiting model, not vitest
        // `expect(...)` — they don't follow this rule's contract.
        if crate::rules::test_assertion_helpers::imports_playwright_test(semantic) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if id.name.as_str() != "test" && id.name.as_str() != "it" {
            return;
        }
        let Some(cb) = call.arguments.get(1) else {
            return;
        };
        let body_span = match cb {
            Argument::ArrowFunctionExpression(a) => a.body.span,
            Argument::FunctionExpression(f) => f
                .body
                .as_ref()
                .map(|b| b.span)
                .unwrap_or_else(|| f.span),
            _ => return,
        };
        let body_text = &ctx.source[body_span.start as usize..body_span.end as usize];
        // Direct matcher chains, short-form `assert(…)`, and arktype's
        // `attest(…)` (the entry point of every `@arktype/attest` assertion)
        // count.
        if body_text.contains(".toBe(")
            || body_text.contains(".toEqual(")
            || body_text.contains(".toThrow(")
            || body_text.contains(".toMatch(")
            || body_text.contains(".toHave")
            || body_text.contains("assert(")
            || body_text.contains("attest(")
        {
            return;
        }
        // Any call whose identifier starts with `expect` or `assert` is
        // treated as an assertion — covers helpers like `expectProblem(…)`,
        // `assertResponse(…)`, in line with eslint-plugin-vitest's
        // `assertFunctionNames` defaults.
        if has_assertion_prefixed_call(body_text) {
            return;
        }
        // Testing Library `getBy*` / `findBy*` queries throw when the element
        // is missing, so they are implicit assertions — a test built only out
        // of them still fails when the UI is wrong.
        if has_throwing_query_call(body_text) {
            return;
        }
        // React render calls (`render`, `renderToString`, `renderHook`, …) throw
        // when the component/hook crashes, so a "does not throw" smoke test built
        // only out of them is a real test, not a silent pass.
        if crate::rules::test_assertion_helpers::has_render_assertion_call(body_text) {
            return;
        }
        // `@ts-expect-error` is TypeScript's compile-time assertion: the
        // compiler itself fails the build if the expected type error is absent.
        // Tests that rely solely on this directive have a valid assertion.
        if body_text.contains("@ts-expect-error") {
            return;
        }
        // A `satisfies` expression in the body marks a type-level test:
        // it passes iff the file compiles, so no runtime assertion is
        // expected. AST-based so the word "satisfies" inside a string
        // literal doesn't count.
        if body_contains_satisfies(semantic, body_span) {
            return;
        }
        // A compile-time type assertion (`type t = Expect<Equal<typeof x, T>>`)
        // passes iff the file compiles, so no runtime assertion is expected.
        // AST-based so the names `Expect`/`Equal` inside a string literal do not
        // count.
        if crate::rules::test_assertion_helpers::body_contains_type_assertion(semantic, body_span) {
            return;
        }
        // Cypress tests assert by chaining `.should(...)`/`.and(...)` onto a `cy`
        // command (`cy.get(x).should(...)`) instead of calling `expect(...)`.
        // AST-based and rooted at the `cy` identifier so a bare `.should(` on an
        // arbitrary object is not mistaken for an assertion.
        if body_contains_cypress_assertion(semantic, body_span) {
            return;
        }
        // A `throw` in the body is a valid assertion mechanism — timing/
        // property/fuzzing tests fail by throwing on a violated condition,
        // which the runner reports as a failure. AST-based so the word
        // "throw" inside a string literal doesn't count.
        if crate::rules::test_assertion_helpers::body_contains_throw(semantic, body_span) {
            return;
        }
        // The test delegates to a caller-supplied callback (`it(name, () =>
        // fn())` inside a wrapper whose `fn` param carries the assertions).
        if crate::rules::test_assertion_helpers::delegates_to_outer_param(node, semantic) {
            return;
        }
        // A promise-returning test whose completion path calls the resolve
        // parameter of an enclosing `new Promise(done => …)` executor asserts
        // by timeout: if `done(...)` is never reached the promise never settles
        // and the runner fails the test. AST-based so it sees the call even when
        // the callback's body is the `new Promise(...)` expression itself.
        if crate::rules::test_assertion_helpers::body_contains_promise_resolve_call(
            semantic, body_span,
        ) {
            return;
        }
        // The test's only assertion may live in a same-file helper function
        // (module or describe scope) called from the body — follow that call
        // edge before flagging.
        if crate::rules::test_assertion_helpers::body_calls_asserting_local_helper(
            body_span, semantic,
        ) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Test has no `expect(...)` / `assert(...)` — it always passes \
                      silently. Add at least one assertion."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
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
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "/tmp/foo.test.ts", &crate::project::ProjectCtx::for_test_with_framework(""), crate::rules::file_ctx::default_static_file_ctx())
    }

    fn run_at(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, path, &crate::project::ProjectCtx::for_test_with_framework(""), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_test_without_expect() {
        let src = r#"test("does nothing", () => { const x = 1 + 1; });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_test_with_expect() {
        let src = r#"test("ok", () => { expect(1).toBe(1); });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_test_with_assert() {
        let src = r#"test("ok", () => { assert(true); });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_test_with_expect_prefixed_helper() {
        // Regression for #79 — helper functions named `expect*` count
        // as assertions even if the body has no literal `expect(`.
        let src = r#"test("ok", async () => { const r = await req(); expectProblem(r, { status: 401 }); });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_test_with_assert_prefixed_helper() {
        let src = r#"test("ok", async () => { const r = await req(); assertResponse(r); });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_match_word_inside_other_identifier() {
        // `inspect(` should NOT be treated as `expect(`.
        let src = r#"test("ok", () => { inspect(x); });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_test_with_expect_type_of() {
        // Regression for #88 — Vitest's `expectTypeOf<X>().toEqualTypeOf<Y>()`
        // is the canonical type-level assertion. The identifier `expectTypeOf`
        // is followed by `<X>` then `()`, not directly by `(`.
        let src = r#"it("narrows correctly", () => { expectTypeOf<"a" | "b">().toEqualTypeOf<"a" | "b">(); });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_test_with_assert_type_generic() {
        // `assertType<T>(value)` — same generic-call shape as expectTypeOf.
        let src = r#"it("ok", () => { assertType<string>(getValue()); });"#;
        assert!(run(src).is_empty());
    }

    // Regression for #260: a test factory delegates its body to a
    // caller-supplied callback parameter — the inline `it` has no assertion
    // but the wrapper's callers provide one.
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

    #[test]
    fn still_flags_call_to_module_helper_without_assertion() {
        let src = r#"
            function setup() {}
            it("x", () => { setup(); });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for #2348: Playwright test files import their runner from
    // `@playwright/test` and use Playwright's own assertion/auto-waiting model
    // (`page.waitForSelector`, `await expect(locator).toBeVisible()`), so they
    // need no vitest `expect()`.
    #[test]
    fn allows_action_only_playwright_test() {
        let src = r#"
            import { test } from '@playwright/test';
            test('go to /', async ({ page }) => {
                await page.goto('/');
                await page.waitForSelector(`text=tRPC user`);
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_vitest_test_without_assertion_no_playwright_import() {
        let src = r#"
            import { test } from 'vitest';
            test('does nothing', () => { const x = 1 + 1; });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for rbaumier/comply#88 (adjacent FP): `expectTypeOf<{ a: string; b: number }>()`
    // — object type parameters contain `;` which previously caused the generic
    // scan to bail early, treating the call as if it had no `>(`.
    #[test]
    fn allows_expect_type_of_with_object_type_param() {
        let src = r#"it("ok", () => { expectTypeOf<{ a: string; b: number }>().toEqualTypeOf<{ a: string; b: number }>(); });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_assert_type_with_object_type_param() {
        let src = r#"it("ok", () => { assertType<{ id: number; name: string }>(getValue()); });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #358 — tests that verify TypeScript type errors via
    // `@ts-expect-error` have no runtime assertions but are still valid tests.
    // The TypeScript compiler itself enforces the expected error.
    #[test]
    fn allows_test_with_ts_expect_error_only() {
        let src = r#"
            it("rejects invalid defaultSort", () => {
                createListQuerySchema({
                    sortColumns: ["id"],
                    // @ts-expect-error — name:asc is not a valid sort column.
                    defaultSort: "name:asc",
                });
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_test_with_multiple_ts_expect_errors() {
        let src = r#"
            it("rejects reserved filter keys", () => {
                createListQuerySchema({
                    filters: {
                        // @ts-expect-error — sort is reserved.
                        sort: z.string(),
                    },
                });
                createListQuerySchema({
                    filters: {
                        // @ts-expect-error — page is reserved.
                        page: z.string(),
                    },
                });
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_test_with_no_assertion_and_no_ts_expect_error() {
        let src = r#"
            it("no assertion here", () => {
                createListQuerySchema({ sortColumns: ["id"], defaultSort: "id:asc" });
            });
        "#;
        assert_eq!(run(src).len(), 1);
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
    fn still_flags_test_mentioning_satisfies_in_string_only() {
        let src = r#"it("x", () => { log("this satisfies nothing"); });"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for #1078 — vitest re-exports an `assert` object (chai-backed);
    // `assert.deepStrictEqual(...)` / `assert.ok(...)` / `assert.throws(...)`
    // are member-call assertions, not direct `assert(...)` calls.
    #[test]
    fn allows_test_with_assert_member_call() {
        let src =
            r#"it("ok", () => { const m = f(); assert.deepStrictEqual(m, expected); });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_test_with_assert_ok_and_throws() {
        let src = r#"it("ok", () => { assert.ok(value); assert.throws(() => fn()); });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #1195 — Effect-TS uses `node:assert` member calls such as
    // `assert.strictEqual(...)` as the sole assertion across thousands of tests.
    // The member-call receiver `assert` is the assertion, so the body must not
    // be flagged.
    #[test]
    fn allows_test_with_assert_strict_equal() {
        let src = r#"it("adds", () => { assert.strictEqual(add(2, 18), 20); });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Negative-space guard for #1195: a test whose body has no `expect`, no
    // bare `assert`, and no `assert.<method>` member call must still fire
    // exactly one diagnostic.
    #[test]
    fn still_flags_test_without_expect_or_assert_member_call() {
        let src = r#"it("adds", () => { const sum = add(2, 18); log(sum); });"#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // Member access on an `assert`-prefixed object that is NOT a call (a
    // property read) must not be mistaken for an assertion.
    #[test]
    fn still_flags_test_with_assert_member_property_read() {
        let src = r#"it("x", () => { const n = assert.length; log(n); });"#;
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
        let src = r#"it("does nothing", () => { const x = 1 + 1; });"#;
        assert_eq!(run_at(src, "/tmp/feature.spec.ts").len(), 1);
    }

    // Regression for #1389 — Testing Library `getBy*` / `findBy*` queries throw
    // when the element is missing, so they are implicit assertions.
    #[test]
    fn allows_test_with_testing_library_get_and_find_queries() {
        let src = r#"
            it("should respect requests after key has changed", async () => {
                renderWithConfig(<Page />);
                screen.getByText("data:");
                await screen.findByText("data:short request");
                screen.getByText("data:short request");
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_test_with_get_all_by_and_find_all_by() {
        let src = r#"it("ok", () => { screen.getAllByRole("listitem"); });"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // `queryBy*` / `queryAllBy*` return `null` instead of throwing, so they are
    // NOT assertions — a test built only out of them still passes silently.
    #[test]
    fn still_flags_test_with_only_query_by() {
        let src = r#"it("x", () => { screen.queryByText("data:"); });"#;
        assert_eq!(run(src).len(), 1);
    }

    // Word-boundary guard: a custom identifier whose tail spells `getByText`
    // must not be mistaken for the Testing Library query.
    #[test]
    fn still_flags_test_with_custom_identifier_containing_query_name() {
        let src = r#"it("x", () => { const n = config.my_getByText; log(n); });"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for #1391 — Node.js native test runner exposes assertions on
    // the test context: `t.assert.strictEqual(...)`, `t.assert.deepStrictEqual(...)`,
    // `t.assert.ifError(...)`. These throw on failure, so a body relying on them
    // is a real test, not a silent pass.
    #[test]
    fn allows_test_with_node_test_context_assertions() {
        let src = r#"
            test("code should handle null/undefined/float", (t, done) => {
                t.plan(8);
                fastify.inject({ method: "GET", url: "/null" }, (error, res) => {
                    t.assert.ifError(error);
                    t.assert.strictEqual(res.statusCode, 500);
                    t.assert.deepStrictEqual(res.json(), { ok: true });
                });
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // True-positive guard for #1391: a Node.js native test whose body has no
    // assertion at all must still fire.
    #[test]
    fn still_flags_node_test_without_any_assertion() {
        let src = r#"
            test("code should handle null/undefined/float", (t, done) => {
                fastify.inject({ method: "GET", url: "/null" }, (error, res) => {
                    log(res.statusCode);
                });
            });
        "#;
        assert_eq!(run(src).len(), 1);
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
        assert_eq!(run(src).len(), 1);
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

    // Word-boundary guard: a custom identifier whose tail spells `render` must
    // not be mistaken for the React render call.
    #[test]
    fn still_flags_test_with_custom_render_identifier() {
        let src = r#"it("x", () => { customRender(<App />); });"#;
        assert_eq!(run_at(src, "/tmp/x.test.tsx").len(), 1, "{:?}", run_at(src, "/tmp/x.test.tsx"));
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
    // asserts by timeout: if `done` is never reached the test fails. The
    // callback body is the `new Promise(...)` expression itself, so the check
    // must be AST-based.
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

    // Regression for #1713 — vuejs/router's `errors.spec.ts` pattern: a
    // module-level `async function` helper holds the assertions
    // (`await expect(...).rejects.toEqual(...)`, `expect(...).toHaveBeenCalledTimes(...)`),
    // and the test body is only `await testError(...)`.
    #[test]
    fn allows_test_calling_module_async_helper_with_rejects_assertion() {
        let src = r#"
            async function testError(nextArgument, expectedError = undefined, to = "/foo") {
                const { router } = createRouter();
                if (expectedError !== undefined) {
                    await expect(router.push(to)).rejects.toEqual(expectedError);
                }
                expect(afterEach).toHaveBeenCalledTimes(0);
                expect(onError).toHaveBeenCalledTimes(1);
            }
            it("lazy loading reject", async () => {
                await testError(true, "failed", "/async");
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #1713 — the second vuejs/router shape: a module-level
    // helper called with an inline arrow argument. The assertion still lives in
    // the helper body, not in the inline arrow.
    #[test]
    fn allows_test_calling_module_helper_with_inline_arrow_argument() {
        let src = r#"
            async function testNavigation(guard, expectedError) {
                const { router } = createRouter();
                router.beforeEach(guard);
                await expect(router.push("/location")).rejects.toEqual(expectedError);
            }
            it('next("/location") triggers afterEach', async () => {
                await testNavigation(
                    ((to, _from) => {
                        if (to.path === "/location") return;
                        else return "/location";
                    }),
                    undefined
                );
            });
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Negative-space guard for #1713: a module-level helper that contains NO
    // assertion does not launder an empty test — it must still fire.
    #[test]
    fn still_flags_test_calling_module_async_helper_without_assertion() {
        let src = r#"
            async function setupRouter(to) {
                const { router } = createRouter();
                await router.push(to);
            }
            it("navigates", async () => {
                await setupRouter("/async");
            });
        "#;
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
}
