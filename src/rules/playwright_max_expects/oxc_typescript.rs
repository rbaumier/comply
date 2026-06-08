//! playwright-max-expects oxc backend.

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

/// Methods on `test`/`it` that are NOT a single test body: `test.describe`
/// groups tests (its callback aggregates every inner test's expects), and the
/// hooks run shared setup/teardown. Counting expects across them is wrong — the
/// limit is per-test. Test modifiers (`test.only`, `test.skip`, `test.fixme`,
/// ...) still denote a single test and are counted.
const NON_TEST_METHODS: &[&str] =
    &["describe", "beforeEach", "afterEach", "beforeAll", "afterAll", "step", "use"];

fn is_test_callee(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => TEST_FNS.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            let Expression::Identifier(obj) = &member.object else {
                return false;
            };
            TEST_FNS.contains(&obj.name.as_str())
                && !NON_TEST_METHODS.contains(&member.property.name.as_str())
        }
        _ => false,
    }
}

fn count_expects_in_source(source: &str, start: usize, end: usize) -> usize {
    let slice = &source[start..end];
    // Count occurrences of "expect(" as a simple heuristic.
    // This matches the tree-sitter approach of walking the subtree.
    let mut count = 0;
    let mut search_from = 0;
    let bytes = slice.as_bytes();
    while search_from + 7 <= bytes.len() {
        if let Some(pos) = slice[search_from..].find("expect(") {
            let abs = search_from + pos;
            // Ensure `expect` is not part of a larger identifier.
            let before_ok =
                abs == 0 || !bytes[abs - 1].is_ascii_alphanumeric() && bytes[abs - 1] != b'_';
            if before_ok {
                count += 1;
            }
            search_from = abs + 7;
        } else {
            break;
        }
    }
    count
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
        _semantic: &'a oxc_semantic::Semantic<'a>,
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

        let max_expects = ctx.config.threshold("playwright-max-expects", "max", ctx.lang);
        let count = count_expects_in_source(
            ctx.source,
            callback_span.start as usize,
            callback_span.end as usize,
        );
        if count > max_expects {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Too many assertion calls ({count}) — maximum allowed is {max_expects}."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts_with_path;

    const IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run(body: &str) -> Vec<Diagnostic> {
        let src = format!("{IMPORT}{body}");
        run_oxc_ts_with_path(&src, &Check, "e2e/auth.spec.ts")
    }

    #[test]
    fn flags_single_test_over_limit() {
        let body = r#"
            test("too many", async ({ page }) => {
                await expect(page).toHaveURL("/");
                await expect(page).toHaveURL("/");
                await expect(page).toHaveURL("/");
                await expect(page).toHaveURL("/");
                await expect(page).toHaveURL("/");
                await expect(page).toHaveURL("/");
            });
        "#;
        assert_eq!(run(body).len(), 1);
    }

    #[test]
    fn does_not_aggregate_across_describe() {
        // Regression for issues #519 / #568: a describe block with 3 tests, each
        // within the per-test limit, must not be flagged on the describe line.
        let body = r#"
            test.describe("auth", () => {
                test("a", async ({ page }) => {
                    await expect(page).toHaveURL("/login");
                    await expect(page.getByText("x")).toHaveCount(0);
                    await expect(page.getByRole("button")).toBeVisible();
                });
                test("b", async ({ page }) => {
                    await expect(page).toHaveURL("/");
                });
                test("c", async ({ page }) => {
                    await expect(page.getByRole("alert")).toContainText(/E-mail/);
                    await expect(page).toHaveURL("/login");
                });
            });
        "#;
        assert!(run(body).is_empty(), "got {:?}", run(body));
    }

    #[test]
    fn still_flags_test_only_modifier_over_limit() {
        let body = r#"
            test.only("too many", async ({ page }) => {
                await expect(page).toHaveURL("/");
                await expect(page).toHaveURL("/");
                await expect(page).toHaveURL("/");
                await expect(page).toHaveURL("/");
                await expect(page).toHaveURL("/");
                await expect(page).toHaveURL("/");
            });
        "#;
        assert_eq!(run(body).len(), 1);
    }

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";


    fn run_oxc_ts(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_path(&format!("{PW_IMPORT}{source}"), &Check, "login.test.ts")
    }


    #[test]
    fn flags_too_many_expects() {
        let src = "test('many', () => {
            expect(1).toBe(1);
            expect(2).toBe(2);
            expect(3).toBe(3);
            expect(4).toBe(4);
            expect(5).toBe(5);
            expect(6).toBe(6);
        });";
        let d = run_oxc_ts(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-max-expects");
    }


    #[test]
    fn allows_five_expects() {
        let src = "test('ok', () => {
            expect(1).toBe(1);
            expect(2).toBe(2);
            expect(3).toBe(3);
            expect(4).toBe(4);
            expect(5).toBe(5);
        });";
        let d = run_oxc_ts(src);
        assert!(d.is_empty());
    }


    #[test]
    fn handles_deeply_nested_test_body() {
        let mut src = String::from("test('deep', () => {");
        for _ in 0..600 {
            src.push_str("if (true) {");
        }
        src.push_str("expect(1).toBe(1);");
        for _ in 0..600 {
            src.push('}');
        }
        src.push_str("});");

        let d = run_oxc_ts(&src);

        assert!(d.is_empty());
    }
}
