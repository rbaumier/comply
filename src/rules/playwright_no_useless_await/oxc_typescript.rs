//! playwright-no-useless-await OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Locator methods that are synchronous (return Locator, not Promise).
const SYNC_LOCATOR_METHODS: &[&str] = &[
    "and",
    "first",
    "getByAltText",
    "getByLabel",
    "getByPlaceholder",
    "getByRole",
    "getByTestId",
    "getByText",
    "getByTitle",
    "last",
    "locator",
    "nth",
    "or",
];

/// Page/frame methods that are synchronous.
const SYNC_PAGE_METHODS: &[&str] = &["frameLocator", "isClosed", "url", "viewportSize"];

/// Sync expect matchers (non-web-first).
const SYNC_MATCHERS: &[&str] = &[
    "toBe",
    "toBeCloseTo",
    "toBeDefined",
    "toBeFalsy",
    "toBeGreaterThan",
    "toBeGreaterThanOrEqual",
    "toBeInstanceOf",
    "toBeLessThan",
    "toBeLessThanOrEqual",
    "toBeNaN",
    "toBeNull",
    "toBeTruthy",
    "toBeUndefined",
    "toContain",
    "toContainEqual",
    "toEqual",
    "toHaveLength",
    "toHaveProperty",
    "toMatch",
    "toMatchObject",
    "toStrictEqual",
    "toThrow",
    "toThrowError",
];

/// Get method name from a call expression's callee (if it's obj.method(...)).
fn get_method_name<'a>(call: &'a oxc_ast::ast::CallExpression<'a>) -> Option<&'a str> {
    if let Expression::StaticMemberExpression(member) = &call.callee {
        return Some(member.property.name.as_str());
    }
    None
}

/// Check if a call expression is `expect(…).matcher(…)` with a sync matcher.
fn is_sync_expect_chain(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let matcher = member.property.name.as_str();
    if !SYNC_MATCHERS.contains(&matcher) {
        return false;
    }
    // `.resolves` / `.rejects` turn the assertion into a Promise, so the
    // `await` is mandatory even with an otherwise-sync matcher. This is also
    // the exact form `prefer-expect-resolves` mandates.
    if chain_has_async_modifier(&member.object) {
        return false;
    }
    contains_expect_root(&member.object)
}

/// True when the chain between `expect(...)` and the matcher contains a
/// `.resolves` or `.rejects` async modifier.
fn chain_has_async_modifier(expr: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = expr else {
        return false;
    };
    let prop = member.property.name.as_str();
    prop == "resolves" || prop == "rejects" || chain_has_async_modifier(&member.object)
}

fn contains_expect_root(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(call) => {
            if let Expression::Identifier(id) = &call.callee {
                return id.name.as_str() == "expect";
            }
            false
        }
        Expression::StaticMemberExpression(member) => contains_expect_root(&member.object),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AwaitExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AwaitExpression(await_expr) = node.kind() else {
            return;
        };
        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }

        let Expression::CallExpression(call) = &await_expr.argument else {
            return;
        };

        let is_useless = if let Some(method) = get_method_name(call) {
            SYNC_LOCATOR_METHODS.contains(&method) || SYNC_PAGE_METHODS.contains(&method)
        } else {
            false
        } || is_sync_expect_chain(call);

        if !is_useless {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unnecessary await expression. This method does not return a Promise.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts_with_path;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_path(&format!("{PW_IMPORT}{source}"), &Check, "app.spec.ts")
    }

    #[test]
    fn flags_await_sync_expect() {
        assert_eq!(run("await expect(1).toBe(1);").len(), 1);
    }

    #[test]
    fn flags_await_sync_expect_with_not() {
        assert_eq!(run("await expect(1).not.toBe(2);").len(), 1);
    }

    #[test]
    fn allows_await_resolves() {
        // Regression for issue #559: `.resolves` makes the assertion a Promise,
        // so the await is mandatory (and required by `prefer-expect-resolves`).
        assert!(run("await expect(res.json()).resolves.toStrictEqual({ status: 'ok' });").is_empty());
    }

    #[test]
    fn allows_await_rejects() {
        assert!(run("await expect(p).rejects.toThrow('boom');").is_empty());
    }
}
