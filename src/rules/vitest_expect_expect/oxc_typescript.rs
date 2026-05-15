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
        _semantic: &'a oxc_semantic::Semantic<'a>,
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
        // Heuristic: any of these mean the test has an assertion.
        if body_text.contains("expect(")
            || body_text.contains("assert(")
            || body_text.contains(".toBe(")
            || body_text.contains(".toEqual(")
            || body_text.contains(".toThrow(")
            || body_text.contains(".toMatch(")
            || body_text.contains(".toHave")
        {
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
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts_with_path_and_framework;

    fn run(src: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_path_and_framework(src, &Check, "/tmp/foo.test.ts", "")
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
}
