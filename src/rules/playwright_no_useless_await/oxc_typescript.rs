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
    contains_expect_root(&member.object)
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
