//! OxcCheck backend for no-done-callback — flag a `test`/`it` callback whose
//! first parameter is a bare binding identifier (`(done) =>`), the legacy
//! completion-callback protocol. A destructured fixture parameter
//! (`({ page }) =>`) is a fixture bag, not a done callback, so it is not flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BindingPattern, Expression, FormalParameters};
use std::sync::Arc;

pub struct Check;

const TEST_BASES: &[&str] = &["test", "it"];
const TEST_MODIFIERS: &[&str] = &["only", "skip"];

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
        if !ctx.project.has_framework("jest") && !ctx.project.has_framework("mocha") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        if !is_test_callee(&call.callee) {
            return;
        }

        // Second argument is the test callback (function or arrow).
        let Some(callback) = call.arguments.get(1) else {
            return;
        };

        let is_done_style = match callback {
            Argument::ArrowFunctionExpression(arrow) => first_param_is_done_identifier(&arrow.params),
            Argument::FunctionExpression(func) => first_param_is_done_identifier(&func.params),
            _ => false,
        };

        if !is_done_style {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Test callback takes a `done`-style parameter — use async/await instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// A `done`-style callback parameter is a bare binding identifier (`(done) =>`).
/// A Playwright/fixture parameter is an object/array destructuring pattern
/// (`({ page }) =>`), which is a fixture bag, not a completion callback.
fn first_param_is_done_identifier(params: &FormalParameters) -> bool {
    params
        .items
        .first()
        .is_some_and(|p| matches!(p.pattern, BindingPattern::BindingIdentifier(_)))
}

fn is_test_callee(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => TEST_BASES.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            let Expression::Identifier(obj) = &member.object else {
                return false;
            };
            TEST_BASES.contains(&obj.name.as_str())
                && TEST_MODIFIERS.contains(&member.property.name.as_str())
        }
        _ => false,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("jest"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_test_with_done_arrow() {
        assert_eq!(run_on("test('x', (done) => { done(); });").len(), 1);
    }

    #[test]
    fn flags_it_with_done_function_expr() {
        assert_eq!(
            run_on("it('x', function(done) { done(); });").len(),
            1
        );
    }

    #[test]
    fn flags_test_only_with_done() {
        assert_eq!(
            run_on("test.only('x', (done) => { done(); });").len(),
            1
        );
    }

    #[test]
    fn flags_it_skip_with_done() {
        assert_eq!(run_on("it.skip('x', (done) => { done(); });").len(), 1);
    }

    #[test]
    fn flags_test_with_bare_identifier_not_named_done() {
        assert_eq!(run_on("test('x', (cb) => { cb(); });").len(), 1);
    }

    #[test]
    fn allows_async_test() {
        assert!(run_on("test('x', async () => { await doThing(); });").is_empty());
    }

    #[test]
    fn allows_playwright_fixture_destructuring() {
        assert!(run_on("test('x', async ({ page }) => {});").is_empty());
    }

    #[test]
    fn allows_playwright_multi_fixture_destructuring() {
        assert!(run_on("test('x', async ({ page, baseURL }) => {});").is_empty());
    }

    #[test]
    fn allows_it_fixture_destructuring() {
        assert!(run_on("it('x', async ({ page }) => {});").is_empty());
    }

    #[test]
    fn allows_array_destructuring_param() {
        assert!(run_on("test('x', ([a]) => {});").is_empty());
    }

    #[test]
    fn allows_test_with_no_params() {
        assert!(run_on("test('x', () => { expect(1).toBe(1); });").is_empty());
    }

    #[test]
    fn allows_non_test_function_with_param() {
        assert!(run_on("myHelper('x', (arg) => { return arg; });").is_empty());
    }

    #[test]
    fn ignores_projects_without_jest_or_mocha() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "test('x', (done) => { done(); });", "t.ts").is_empty());
    }
}
