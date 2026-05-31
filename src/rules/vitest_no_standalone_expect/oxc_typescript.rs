//! vitest-no-standalone-expect oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Suite-grouping calls whose callback runs at collection time, not test
/// time — an `expect()` directly in their body is genuinely standalone.
const COLLECTION_BLOCKS: &[&str] = &["describe", "suite", "fdescribe", "xdescribe"];

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

/// True when an `expect()` is genuinely standalone: either it has no
/// enclosing function (module scope, runs at import time) or its nearest
/// enclosing function is a `describe`/`suite` callback (collection time).
///
/// Any other enclosing function — a `test`/`it`/hook callback, or a helper
/// invoked from one (custom assertion helper, precondition guard) — is a
/// live test context. The call graph is invisible to a single-file check,
/// so a helper containing `expect()` is assumed to run inside a test.
fn is_standalone_expect<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if matches!(
            ancestor.kind(),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        ) {
            let parent = semantic.nodes().parent_node(ancestor.id());
            if let AstKind::CallExpression(call) = parent.kind() {
                if let Some(name) = callee_base_name(&call.callee) {
                    return COLLECTION_BLOCKS.contains(&name);
                }
            }
            return false;
        }
    }
    true
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
        if !is_standalone_expect(node, semantic) {
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

    // Regression for #515: expect() inside a helper function invoked from a
    // test callback runs as a real assertion and must not be flagged.
    #[test]
    fn allows_expect_in_assertion_helper_issue_515() {
        let src = r#"
            function assertSentenceCase(result) {
                expect(result).toBe(result.toUpperCase());
                expect(result).not.toBe(result.toLowerCase());
            }
            it.each([])(":name", (_label, run) => {
                assertSentenceCase(run());
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_expect_in_async_precondition_helper_issue_515() {
        let src = r#"
            async function createTargetUser(cookie, body) {
                const res = await request(cookie, body);
                expect(res.status).toBe(200);
                return res.body;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_expect_directly_in_describe_body() {
        let src = r#"describe("group", () => { expect(1).toBe(1); });"#;
        assert_eq!(run(src).len(), 1);
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
