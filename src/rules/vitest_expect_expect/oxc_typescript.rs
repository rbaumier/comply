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

/// True when `text` contains a call to a function whose identifier starts
/// with `expect` or `assert`. The identifier must be word-boundary-anchored
/// on the left so we don't match `inspect(` or `reassert(`. A TypeScript
/// type-argument list (`<...>`) between the identifier and the call's `(`
/// is allowed — `expectTypeOf<X>()` is a runtime call.
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
        if !is_test_file(ctx.path) {
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
        // The test delegates to a caller-supplied callback (`it(name, () =>
        // fn())` inside a wrapper whose `fn` param carries the assertions).
        if crate::rules::test_assertion_helpers::delegates_to_outer_param(node, semantic) {
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
}
