//! vitest-no-done-callback oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BindingPattern, Expression, FormalParameters};
use std::sync::Arc;

pub struct Check;

const TEST_FUNCTIONS: &[&str] = &[
    "test",
    "it",
    "beforeEach",
    "afterEach",
    "beforeAll",
    "afterAll",
];

fn has_done_param(params: &FormalParameters) -> bool {
    params.items.iter().any(|p| match &p.pattern {
        BindingPattern::BindingIdentifier(id) => id.name.as_str() == "done",
        _ => false,
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["done"])
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
        let callee_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            Expression::StaticMemberExpression(m) => m.property.name.as_str(),
            _ => return,
        };
        if !TEST_FUNCTIONS.contains(&callee_name) {
            return;
        }
        // Last argument is the callback.
        let Some(arg) = call.arguments.last() else {
            return;
        };
        let params = match arg {
            Argument::ArrowFunctionExpression(a) => &a.params,
            Argument::FunctionExpression(f) => &f.params,
            _ => return,
        };
        if !has_done_param(params) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`done` callback is a Jest legacy pattern — Vitest will never \
                      finish the test. Return a Promise or mark the callback async."
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
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_done_callback_in_test() {
        let src = r#"test("x", (done) => { setTimeout(done, 100); });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_done_in_beforeEach() {
        let src = r#"beforeEach((done) => { setup(done); });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_promise_callback() {
        let src = r#"test("x", async () => { await waitFor(); });"#;
        assert!(run(src).is_empty());
    }
}
