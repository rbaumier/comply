//! playwright-expect-expect oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const TEST_FNS: &[&str] = &["test", "it"];

/// Member-expression properties on `test`/`it` that are NOT individual test
/// cases: `test.describe(...)` groups cases and the hooks run setup/teardown.
/// Their callbacks legitimately contain no `expect(...)` of their own, so they
/// must not be checked for assertions. Modifiers that still define a single
/// case (`test.only`, `test.skip`, `test.concurrent`, …) are not listed and
/// remain subject to the check.
const NON_TEST_CASE_PROPS: &[&str] =
    &["describe", "beforeEach", "afterEach", "beforeAll", "afterAll"];

fn is_test_callee(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => TEST_FNS.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            let Expression::Identifier(obj) = &member.object else {
                return false;
            };
            TEST_FNS.contains(&obj.name.as_str())
                && !NON_TEST_CASE_PROPS.contains(&member.property.name.as_str())
        }
        _ => false,
    }
}

fn callback_contains_expect(source: &str, start: usize, end: usize) -> bool {
    let slice = &source[start..end];
    let bytes = slice.as_bytes();
    let mut search_from = 0;
    while search_from + 7 <= bytes.len() {
        if let Some(pos) = slice[search_from..].find("expect(") {
            let abs = search_from + pos;
            let before_ok =
                abs == 0 || !bytes[abs - 1].is_ascii_alphanumeric() && bytes[abs - 1] != b'_';
            if before_ok {
                return true;
            }
            search_from = abs + 7;
        } else {
            break;
        }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }
        if !is_test_callee(&call.callee) {
            return;
        }

        // The callback is typically the last argument.
        if call.arguments.is_empty() {
            return;
        }
        let last_arg = &call.arguments[call.arguments.len() - 1];
        let callback_span = match last_arg {
            Argument::ArrowFunctionExpression(arrow) => arrow.span,
            Argument::FunctionExpression(func) => func.span(),
            _ => return,
        };

        if callback_contains_expect(
            ctx.source,
            callback_span.start as usize,
            callback_span.end as usize,
        ) {
            return;
        }
        // The assertion may be delegated to a same-file helper whose body wraps
        // `expect(...)` (e.g. `await expectSelectedTab(tabs, ...)`), or named by
        // the `expect`/`assert` convention even when imported. Follow both
        // signals before flagging.
        if crate::rules::test_assertion_helpers::body_calls_asserting_local_helper(
            callback_span,
            semantic,
        ) || crate::rules::test_assertion_helpers::body_contains_assertion_prefixed_call(
            callback_span,
            semantic,
        ) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Test has no assertions.".into(),
            severity: Severity::Warning,
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

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, &format!("{PW_IMPORT}{src}"), "login.spec.ts")
    }

    #[test]
    fn flags_test_without_expect() {
        let d = run("test('should work', () => { const x = 1; });");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-expect-expect");
    }

    #[test]
    fn allows_test_with_expect() {
        let d = run("test('should work', () => { expect(1).toBe(1); });");
        assert!(d.is_empty(), "{d:?}");
    }

    // Regression for #1397 — `test.describe(...)` is the grouping container,
    // not an individual test case; the assertions live in the nested `test(...)`
    // calls. Flagging the describe block for "no assertions" is a false positive.
    #[test]
    fn does_not_flag_describe_block() {
        let d = run(
            "test.describe('Keyboard shortcuts', () => {\n  \
                test.beforeEach(({page}) => initialize({page}));\n  \
                test('Can use bold format', async ({page}) => {\n    \
                    expect(await getSelectedFormat(page)).toBe('bold');\n  \
                });\n\
            });",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn does_not_flag_hooks() {
        let d = run(
            "test.beforeEach(({page}) => initialize({page}));\n\
             test.afterEach(({page}) => cleanup({page}));\n\
             test.beforeAll(() => setup());\n\
             test.afterAll(() => teardown());",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // True positive must still fire: a real `test(...)` case with no assertion.
    #[test]
    fn still_flags_real_test_without_assertion_inside_describe() {
        let d = run(
            "test.describe('group', () => {\n  \
                test('does nothing', async ({page}) => { await page.click('#btn'); });\n\
            });",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    // Regression for #2138 — withastro/starlight's `basics.test.ts` pattern: the
    // test body's only assertion lives in a file-scope helper
    // (`expectSelectedTab`) that wraps `expect(...).toHaveAttribute(...)`. The
    // body has no literal `expect(`, only calls to the helper.
    #[test]
    fn allows_test_delegating_to_file_scope_helper_with_expect() {
        let d = run(
            "async function expectSelectedTab(tabs, syncKey, panelText) {\n  \
                await expect(tabs.getByRole('tab')).toHaveAttribute('data-sync-key', syncKey);\n  \
                await expect(tabs.getByRole('tabpanel')).toHaveText(panelText);\n\
             }\n\
             test('syncs tabs', async ({page}) => {\n  \
                const tabs = page.locator('starlight-tabs');\n  \
                await expectSelectedTab(tabs, 'pnpm', 'pnpm command');\n\
             });",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // Regression for #2138 — the assertion-name convention also covers the case
    // where the `expect`-prefixed helper is not defined in this file (imported).
    #[test]
    fn allows_test_delegating_to_expect_prefixed_callee() {
        let d = run("test('shows banner', async ({page}) => { await expectVisible(page, '#banner'); });");
        assert!(d.is_empty(), "{d:?}");
    }

    // Negative-space guard for #2138: a test whose body calls only a
    // non-asserting helper (no `expect` inside, non-assertion name) must STILL
    // be flagged.
    #[test]
    fn still_flags_test_calling_non_asserting_helper() {
        let d = run(
            "async function setup(page) {\n  \
                await page.goto('/');\n\
             }\n\
             test('does nothing', async ({page}) => {\n  \
                await setup(page);\n\
             });",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }
}
