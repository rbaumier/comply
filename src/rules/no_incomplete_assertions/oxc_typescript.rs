//! no-incomplete-assertions OXC backend — flag `expect()` calls without a matcher.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const MATCHERS: &[&str] = &[
    "toBe",
    "toEqual",
    "toMatch",
    "toThrow",
    "toContain",
    "toBeTruthy",
    "toBeFalsy",
    "toBeNull",
    "toBeUndefined",
    "toBeDefined",
    "toBeGreaterThan",
    "toBeLessThan",
    "toBeInstanceOf",
    "toHaveBeenCalled",
    "toHaveBeenCalledWith",
    "toHaveLength",
    "toHaveProperty",
    "toMatchObject",
    "toMatchSnapshot",
    "toMatchInlineSnapshot",
    "toStrictEqual",
    "resolves",
    "rejects",
    "toBeCloseTo",
    "toBeNaN",
];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.")
        || s.contains(".spec.")
        || s.contains("__tests__")
        || s.contains("_test.")
}

fn is_expect_call(expr: &Expression) -> bool {
    if let Expression::CallExpression(call) = expr {
        if let Expression::Identifier(ident) = &call.callee {
            return ident.name.as_str() == "expect";
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["expect"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::ExpressionStatement(stmt) = node.kind() else {
                continue;
            };

            let expr = &stmt.expression;

            // Case 1: bare `expect(x);`
            if is_expect_call(expr) {
                let span = oxc_span::GetSpan::span(expr);
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Incomplete assertion — `expect()` without a matcher tests nothing."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
                continue;
            }

            // Case 2: `expect(x).not;` — member expression without a call
            if let Expression::StaticMemberExpression(member) = expr {
                let prop = member.property.name.as_str();
                if MATCHERS.contains(&prop) {
                    continue;
                }
                // Check if the object is `expect(...)`.
                if is_expect_call(&member.object) {
                    let span = oxc_span::GetSpan::span(expr);
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message:
                            "Incomplete assertion — `expect()` without a matcher tests nothing."
                                .into(),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
        }

        diagnostics
    }
}
