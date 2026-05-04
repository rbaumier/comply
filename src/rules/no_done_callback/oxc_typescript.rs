//! OxcCheck backend for no-done-callback — flag `test`/`it` callbacks
//! that take a `done`-style parameter.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
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

        // Second argument should be a function/arrow with at least one param.
        let Some(callback) = call.arguments.get(1) else {
            return;
        };

        let has_param = match callback {
            Argument::ArrowFunctionExpression(arrow) => !arrow.params.items.is_empty(),
            Argument::FunctionExpression(func) => !func.params.items.is_empty(),
            _ => false,
        };

        if !has_param {
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "jest")
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
    fn allows_async_test() {
        assert!(run_on("test('x', async () => { await doThing(); });").is_empty());
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
        assert!(crate::rules::test_helpers::run_oxc_ts("test('x', (done) => { done(); });", &Check).is_empty());
    }
}
