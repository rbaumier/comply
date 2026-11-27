//! testing-no-elapsed-time-bound — OXC backend.

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::{
    byte_offset_to_line_col, const_init, file_uses_fake_timers, is_use_fake_timers,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BinaryOperator, CallExpression, Expression, Statement};
use oxc_semantic::{AstNode, NodeId, Semantic};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let is_timed_test_file = ctx.file.path_segments.in_test_dir
            && !ctx.file.in_benchmark_dir()
            && ctx.source_contains(".now()");
        if !is_timed_test_file {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Some(subject) = upper_bound_subject(call) else {
            return;
        };
        if !is_clock_delta(subject, semantic) {
            return;
        }
        let is_under_fake_timers =
            file_uses_fake_timers(ctx.source) && runs_under_fake_timers(node.id(), semantic);
        if is_under_fake_timers {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Upper bound on elapsed wall-clock time: assert the order of events, \
                      count steps, or take a wide margin."
                .into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

/// `x` of `expect(x).toBeLessThan(..)` or `expect(x).not.toBeGreaterThan(..)`.
fn upper_bound_subject<'a>(call: &'a CallExpression<'a>) -> Option<&'a Expression<'a>> {
    let Expression::StaticMemberExpression(matcher) = &call.callee else {
        return None;
    };
    let (expect_call, is_negated) = match &matcher.object {
        Expression::CallExpression(expect_call) => (expect_call, false),
        Expression::StaticMemberExpression(not) if not.property.name == "not" => {
            let Expression::CallExpression(expect_call) = &not.object else {
                return None;
            };
            (expect_call, true)
        }
        _ => return None,
    };
    let is_upper_bound = match matcher.property.name.as_str() {
        "toBeLessThan" | "toBeLessThanOrEqual" => !is_negated,
        "toBeGreaterThan" | "toBeGreaterThanOrEqual" => is_negated,
        _ => false,
    };
    let Expression::Identifier(expect) = &expect_call.callee else {
        return None;
    };
    if !is_upper_bound || expect.name != "expect" {
        return None;
    }
    expect_call
        .arguments
        .first()
        .and_then(Argument::as_expression)
}

/// `a - b` reading the clock, inline or via a `const`.
fn is_clock_delta(expr: &Expression, semantic: &Semantic) -> bool {
    match expr.without_parentheses() {
        Expression::Identifier(id) => {
            const_init(id, semantic).is_some_and(|init| is_clock_subtraction(init, semantic))
        }
        expr => is_clock_subtraction(expr, semantic),
    }
}

fn is_clock_subtraction(expr: &Expression, semantic: &Semantic) -> bool {
    let Expression::BinaryExpression(binary) = expr.without_parentheses() else {
        return false;
    };
    binary.operator == BinaryOperator::Subtraction
        && (is_clock_operand(&binary.left, semantic) || is_clock_operand(&binary.right, semantic))
}

/// A clock read, inline or via a `const`.
fn is_clock_operand(expr: &Expression, semantic: &Semantic) -> bool {
    match expr.without_parentheses() {
        Expression::Identifier(id) => {
            const_init(id, semantic).is_some_and(|init| is_clock_read(init, semantic))
        }
        expr => is_clock_read(expr, semantic),
    }
}

/// Global `Date.now()` or `performance.now()`.
fn is_clock_read(expr: &Expression, semantic: &Semantic) -> bool {
    let Expression::CallExpression(call) = expr.without_parentheses() else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let Expression::Identifier(clock) = &member.object else {
        return false;
    };
    member.property.name == "now"
        && matches!(clock.name.as_str(), "Date" | "performance")
        && semantic.is_reference_to_global_variable(clock)
}

/// An enclosing scope's statement or hook enables fake timers.
fn runs_under_fake_timers(node_id: NodeId, semantic: &Semantic) -> bool {
    semantic
        .nodes()
        .ancestor_kinds(node_id)
        .any(|kind| match kind {
            AstKind::FunctionBody(body) => scope_enables_fake_timers(&body.statements),
            AstKind::Program(program) => scope_enables_fake_timers(&program.body),
            _ => false,
        })
}

fn scope_enables_fake_timers(statements: &[Statement]) -> bool {
    statements
        .iter()
        .filter_map(statement_call)
        .any(|call| is_use_fake_timers(call) || is_fake_timer_hook(call))
}

/// `beforeEach(() => vi.useFakeTimers())`, likewise `beforeAll`.
fn is_fake_timer_hook(call: &CallExpression) -> bool {
    let Expression::Identifier(hook) = &call.callee else {
        return false;
    };
    matches!(hook.name.as_str(), "beforeEach" | "beforeAll")
        && call.arguments.iter().any(|argument| {
            let statements = match argument
                .as_expression()
                .map(Expression::without_parentheses)
            {
                Some(Expression::ArrowFunctionExpression(arrow)) => &arrow.body.statements,
                Some(Expression::FunctionExpression(function)) => match &function.body {
                    Some(body) => &body.statements,
                    None => return false,
                },
                _ => return false,
            };
            statements
                .iter()
                .filter_map(statement_call)
                .any(is_use_fake_timers)
        })
}

fn statement_call<'a>(statement: &'a Statement<'a>) -> Option<&'a CallExpression<'a>> {
    let Statement::ExpressionStatement(statement) = statement else {
        return None;
    };
    let Expression::CallExpression(call) = statement.expression.without_parentheses() else {
        return None;
    };
    Some(call)
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
    use crate::rules::test_helpers::run_rule_gated;

    const TEST_PATH: &str = "src/timer.test.ts";
    const DATE_DELTA: &str = "it('x', () => { const start = Date.now(); run(); expect(Date.now() - start).toBeLessThan(100) })";

    fn count(src: &str, path: &str) -> usize {
        run_rule_gated(&Check, src, path).len()
    }

    #[test]
    fn flags_upper_bound_matchers_on_clock_delta() {
        let diagnostics = run_rule_gated(&Check, DATE_DELTA, TEST_PATH);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, super::super::META.severity);
        for matcher in [
            "toBeLessThanOrEqual",
            "not.toBeGreaterThan",
            "not.toBeGreaterThanOrEqual",
        ] {
            let src = format!(
                "it('x', () => {{ const s = Date.now(); expect(Date.now() - s).{matcher}(9) }})"
            );
            assert_eq!(count(&src, TEST_PATH), 1, "{matcher}");
        }
        let reversed = "it('x', () => { expect(start - performance.now()).toBeLessThan(5) })";
        assert_eq!(count(reversed, TEST_PATH), 1);
        let shadowed = "const performance = fake;\n\
                        it('x', () => { expect(performance.now() - t).toBeLessThan(5) })";
        assert_eq!(count(shadowed, TEST_PATH), 0);
    }

    #[test]
    fn flags_delta_through_local_const() {
        let tracked = "it('x', () => { const elapsed = performance.now() - t0; \
                       expect(elapsed).not.toBeGreaterThan(50) })";
        assert_eq!(count(tracked, TEST_PATH), 1);
        let held_reads = "it('x', () => { const t0 = performance.now(); run(); \
                          const t1 = performance.now(); expect(t1 - t0).toBeLessThan(50) })";
        assert_eq!(count(held_reads, TEST_PATH), 1);
        let held_delta = "it('x', () => { const start = Date.now(); const end = Date.now(); \
                          const elapsed = end - start; expect(elapsed).toBeLessThan(50) })";
        assert_eq!(count(held_delta, TEST_PATH), 1);
        let computed =
            "it('x', () => { const elapsed = compute(); expect(elapsed).toBeLessThan(50) })";
        assert_eq!(count(computed, TEST_PATH), 0);
    }

    #[test]
    fn ignores_lower_bounds() {
        for matcher in [
            "toBeGreaterThanOrEqual",
            "toBeGreaterThan",
            "not.toBeLessThan",
        ] {
            let src = format!(
                "it('x', () => {{ const s = Date.now(); expect(Date.now() - s).{matcher}(100) }})"
            );
            assert_eq!(count(&src, TEST_PATH), 0, "{matcher}");
        }
    }

    #[test]
    fn ignores_fake_timer_scopes() {
        let in_test = "it('x', () => { vi.useFakeTimers(); const s = Date.now(); \
                       expect(Date.now() - s).toBeLessThan(100) })";
        let in_describe_hook = format!(
            "describe('d', () => {{ beforeEach(() => jest.useFakeTimers()); {DATE_DELTA} }})"
        );
        let file_level_hook = format!("beforeEach(() => {{ vi.useFakeTimers() }});\n{DATE_DELTA}");
        assert_eq!(count(in_test, TEST_PATH), 0);
        assert_eq!(count(&in_describe_hook, TEST_PATH), 0);
        assert_eq!(count(&file_level_hook, TEST_PATH), 0);
    }

    #[test]
    fn flags_tests_outside_fake_timer_scope() {
        let siblings = format!(
            "describe('a', () => {{ beforeEach(() => vi.useFakeTimers()); {DATE_DELTA} }});\n\
             describe('b', () => {{ {DATE_DELTA} }})"
        );
        let diagnostics = run_rule_gated(&Check, &siblings, TEST_PATH);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 2);
        let parent = format!(
            "describe('p', () => {{\n\
             describe('n', () => {{ beforeEach(() => vi.useFakeTimers()); {DATE_DELTA} }});\n\
             {DATE_DELTA}\n}})"
        );
        let diagnostics = run_rule_gated(&Check, &parent, TEST_PATH);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 3);
    }

    #[test]
    fn ignores_benchmark_and_non_test_files() {
        assert_eq!(count(DATE_DELTA, "tests/perf.bench.ts"), 0);
        assert_eq!(count(DATE_DELTA, "src/timing.ts"), 0);
    }

    #[test]
    fn message_suggests_order_or_margin() {
        let diagnostics = run_rule_gated(&Check, DATE_DELTA, TEST_PATH);
        assert!(diagnostics[0].message.contains("order"));
        assert!(diagnostics[0].message.contains("margin"));
    }
}
