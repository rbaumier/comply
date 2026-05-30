//! vitest-no-standalone-expect oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const TEST_BLOCKS: &[&str] = &[
    "test",
    "it",
    "describe",
    "suite",
    "beforeAll",
    "beforeEach",
    "afterAll",
    "afterEach",
];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__")
}

/// Extract the base function name from a call expression's callee.
/// Returns `None` for patterns that don't resolve to a single identifier.
fn callee_base_name<'a>(callee: &'a Expression<'a>) -> Option<&'a str> {
    match callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(m) => {
            if let Expression::Identifier(obj) = &m.object {
                Some(obj.name.as_str())
            } else {
                None
            }
        }
        // it.each(array)("title", cb) — callee is the result of `it.each(array)`
        Expression::CallExpression(inner) => callee_base_name(&inner.callee),
        _ => None,
    }
}

/// Walk up ancestors looking for a CallExpression whose callee is one
/// of the known test blocks. Returns true if found.
fn inside_test_block<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::CallExpression(call) = ancestor.kind() {
            if let Some(name) = callee_base_name(&call.callee) {
                if TEST_BLOCKS.contains(&name) {
                    return true;
                }
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["expect("])
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
        if id.name.as_str() != "expect" {
            return;
        }
        if inside_test_block(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`expect(...)` outside any test block — it runs at import time, \
                      not as part of a test. Move it into `test(...)` or `beforeAll(...)`."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(src, &Check, "policy.test.ts")
    }

    #[test]
    fn flags_expect_at_top_level() {
        let src = r#"expect(1).toBe(1);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_expect_inside_it() {
        let src = r#"it("x", () => { expect(1).toBe(1); });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_expect_inside_it_each() {
        let src = r#"it.each([1, 2])("n=%i", (n) => { expect(n).toBeGreaterThan(0); });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_expect_inside_test_each() {
        let src = r#"test.each([1, 2])("n=%i", (n) => { expect(n).toBeGreaterThan(0); });"#;
        assert!(run(src).is_empty());
    }

    // Regression for #347: it.each nested inside describe.each was falsely flagged.
    #[test]
    fn no_fp_it_each_nested_in_describe_each() {
        let src = r#"
            describe.each([["a"], ["b"]])("%s", (_label) => {
                it("plain", () => {
                    expect(true).toBe(true);
                });
                it.each([["x"], ["y"]])("each %s", (_s) => {
                    expect(true).toBe(true);
                });
            });
        "#;
        assert!(run(src).is_empty());
    }
}
