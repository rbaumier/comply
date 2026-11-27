//! testing-no-sleep-in-test — OXC backend.

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::{
    byte_offset_to_line_col, file_uses_fake_timers, local_callee_body, resolves_to_import_from,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, CallExpression, Expression, FunctionBody, IdentifierReference, NewExpression,
    Statement,
};
use oxc_semantic::{AstNode, NodeId, Semantic};
use oxc_span::GetSpan;
use std::sync::Arc;

const TIMERS_PROMISES: &[&str] = &["node:timers/promises", "timers/promises"];
const TEST_CALLEES: &[&str] = &[
    "it",
    "test",
    "beforeEach",
    "afterEach",
    "beforeAll",
    "afterAll",
];
const RUN_NOW_FUNCTIONS: &[&str] = &["act", "waitFor"];
const RUN_NOW_METHODS: &[&str] = &["then", "catch", "finally", "step"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AwaitExpression, AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !["setTimeout", "sleep", "scheduler"]
            .iter()
            .any(|needle| ctx.source_contains(needle))
            || file_uses_fake_timers(ctx.source)
        {
            return;
        }
        let slept = match node.kind() {
            AstKind::AwaitExpression(await_expr) if is_sleep(&await_expr.argument, semantic) => {
                await_expr.argument.span()
            }
            AstKind::CallExpression(call)
                if is_bun_method(&call.callee, "sleepSync", semantic)
                    && !is_zero_delay(call.arguments.first()) =>
            {
                call.span
            }
            _ => return,
        };
        if !runs_in_test_body(node.id(), semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, slept.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Fixed sleep in a test: wait on the condition or use fake timers instead."
                .into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

/// Awaiting `expr` sleeps: built-in sleep or same-file sleep helper.
fn is_sleep(expr: &Expression, semantic: &Semantic) -> bool {
    let expr = expr.without_parentheses();
    if let Expression::CallExpression(call) = expr
        && let Expression::Identifier(callee) = &call.callee
        && let Some((body, _, is_expression)) = local_callee_body(callee, semantic)
    {
        // `pause(0)` flushes the macrotask queue like `setTimeout(r, 0)`.
        return !call.arguments.first().is_some_and(is_zero_literal)
            && returned_expression(body, is_expression)
                .is_some_and(|returned| is_builtin_sleep(returned, semantic));
    }
    is_builtin_sleep(expr, semantic)
}

fn returned_expression<'a>(
    body: &'a FunctionBody<'a>,
    is_expression: bool,
) -> Option<&'a Expression<'a>> {
    match body.statements.last()? {
        Statement::ExpressionStatement(statement) if is_expression => Some(&statement.expression),
        Statement::ReturnStatement(statement) => statement.argument.as_ref(),
        _ => None,
    }
}

fn is_builtin_sleep(expr: &Expression, semantic: &Semantic) -> bool {
    match expr.without_parentheses() {
        Expression::NewExpression(new_expr) => is_promise_timeout(new_expr, semantic),
        Expression::CallExpression(call) => {
            is_sleep_call(call, semantic) && !is_zero_delay(call.arguments.first())
        }
        _ => false,
    }
}

/// `new Promise(r => setTimeout(r, ms))`, `ms` non-zero.
fn is_promise_timeout(new_expr: &NewExpression, semantic: &Semantic) -> bool {
    let Expression::Identifier(callee) = &new_expr.callee else {
        return false;
    };
    let Some(Expression::ArrowFunctionExpression(executor)) =
        new_expr.arguments.first().and_then(Argument::as_expression)
    else {
        return false;
    };
    callee.name.as_str() == "Promise"
        && executor.body.statements.iter().any(|statement| {
            let Statement::ExpressionStatement(statement) = statement else {
                return false;
            };
            let Expression::CallExpression(call) = &statement.expression else {
                return false;
            };
            matches!(&call.callee, Expression::Identifier(id)
                if id.name.as_str() == "setTimeout" && is_global(id, semantic))
                && !is_zero_delay(call.arguments.get(1))
        })
}

/// `Bun.sleep`, `scheduler.wait`, or `setTimeout` from `timers/promises`.
fn is_sleep_call(call: &CallExpression, semantic: &Semantic) -> bool {
    match &call.callee {
        Expression::Identifier(id) => imports_timers_export(id, "setTimeout", semantic),
        Expression::StaticMemberExpression(member) => {
            let Expression::Identifier(object) = &member.object else {
                return false;
            };
            match member.property.name.as_str() {
                "sleep" => is_bun_method(&call.callee, "sleep", semantic),
                "wait" => {
                    imports_timers_export(object, "scheduler", semantic)
                        || (object.name.as_str() == "scheduler" && is_global(object, semantic))
                }
                "setTimeout" => matches!(
                    timers_import(object, semantic),
                    Some(AstKind::ImportNamespaceSpecifier(_))
                ),
                _ => false,
            }
        }
        _ => false,
    }
}

fn is_bun_method(callee: &Expression, method: &str, semantic: &Semantic) -> bool {
    let Expression::StaticMemberExpression(member) = callee else {
        return false;
    };
    matches!(&member.object, Expression::Identifier(object)
        if object.name.as_str() == "Bun" && is_global(object, semantic))
        && member.property.name.as_str() == method
}

fn is_global(id: &IdentifierReference, semantic: &Semantic) -> bool {
    id.reference_id.get().is_some_and(|reference| {
        semantic
            .scoping()
            .get_reference(reference)
            .symbol_id()
            .is_none()
    })
}

/// `id` imports `export` from `timers/promises`, alias allowed.
fn imports_timers_export(id: &IdentifierReference, export: &str, semantic: &Semantic) -> bool {
    matches!(timers_import(id, semantic), Some(AstKind::ImportSpecifier(spec))
        if spec.imported.name().as_str() == export)
}

/// The import specifier binding `id`, when it imports from `timers/promises`.
fn timers_import<'a>(id: &IdentifierReference, semantic: &'a Semantic<'a>) -> Option<AstKind<'a>> {
    if !resolves_to_import_from(id, semantic, TIMERS_PROMISES) {
        return None;
    }
    let scoping = semantic.scoping();
    let symbol = scoping.get_reference(id.reference_id.get()?).symbol_id()?;
    Some(semantic.nodes().kind(scoping.symbol_declaration(symbol)))
}

/// No delay means zero. Zero only flushes the macrotask queue.
fn is_zero_delay(arg: Option<&Argument>) -> bool {
    arg.is_none_or(is_zero_literal)
}

fn is_zero_literal(arg: &Argument) -> bool {
    arg.as_expression().is_some_and(Expression::is_number_0)
}

/// `node_id` runs in a test or hook body. Poll loops excluded.
fn runs_in_test_body(node_id: NodeId, semantic: &Semantic) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node_id) {
        match ancestor.kind() {
            kind if is_loop(&kind) && loop_decides_exit(ancestor, semantic) => return false,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                // A callback handed to anything else may run later, e.g. simulated latency.
                let AstKind::CallExpression(call) = nodes.parent_node(ancestor.id()).kind() else {
                    return false;
                };
                if !is_run_now_callee(&call.callee) {
                    return is_test_callee(&call.callee);
                }
            }
            _ => {}
        }
    }
    false
}

fn is_test_callee(callee: &Expression) -> bool {
    let root = match callee {
        Expression::StaticMemberExpression(member) => &member.object,
        other => other,
    };
    matches!(root, Expression::Identifier(id) if TEST_CALLEES.contains(&id.name.as_str()))
}

fn is_run_now_callee(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => RUN_NOW_FUNCTIONS.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            RUN_NOW_METHODS.contains(&member.property.name.as_str())
        }
        _ => false,
    }
}

fn is_loop(kind: &AstKind) -> bool {
    matches!(
        kind,
        AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_)
            | AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
    )
}

/// The loop's condition or a `break`/`return` decides its exit.
fn loop_decides_exit(loop_node: &AstNode, semantic: &Semantic) -> bool {
    let condition = match loop_node.kind() {
        AstKind::WhileStatement(statement) => Some(&statement.test),
        AstKind::DoWhileStatement(statement) => Some(&statement.test),
        AstKind::ForStatement(statement) => statement.test.as_ref(),
        _ => None,
    };
    if condition.is_some_and(|test| {
        !matches!(test.without_parentheses(), Expression::BooleanLiteral(literal) if literal.value)
    }) {
        return true;
    }
    let loop_span = loop_node.kind().span();
    semantic.nodes().iter().any(|node| {
        matches!(
            node.kind(),
            AstKind::BreakStatement(_) | AstKind::ReturnStatement(_)
        ) && loop_span.contains_inclusive(node.kind().span())
            && exits_loop(node, loop_node.id(), semantic)
    })
}

/// True when the `break`/`return` at `exit` leaves the loop `loop_id`.
fn exits_loop(exit: &AstNode, loop_id: NodeId, semantic: &Semantic) -> bool {
    let is_unlabeled_break =
        matches!(exit.kind(), AstKind::BreakStatement(statement) if statement.label.is_none());
    for ancestor in semantic.nodes().ancestors(exit.id()) {
        if ancestor.id() == loop_id {
            return true;
        }
        let kind = ancestor.kind();
        let is_function = matches!(
            kind,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        );
        let is_break_target = is_loop(&kind) || matches!(kind, AstKind::SwitchStatement(_));
        if is_function || (is_unlabeled_break && is_break_target) {
            return false;
        }
    }
    false
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "x.test.ts")
    }

    #[test]
    fn flags_fixed_sleeps_in_test_and_hook_bodies() {
        let globals_and_aliases = r"
import { setTimeout as delay } from 'node:timers/promises';
import * as timers from 'timers/promises';
beforeEach(async () => { await Bun.sleep(100); });
it('waits', async () => {
  await new Promise(r => setTimeout(r, 500));
  await delay(100);
  await timers.setTimeout(100);
  await Bun.sleep(100);
  await scheduler.wait(100);
  Bun.sleepSync(100);
});
";
        assert_eq!(run(globals_and_aliases).len(), 7);
        let named_import = r"
import { setTimeout } from 'node:timers/promises';
it('waits', async () => { await setTimeout(100); });
";
        assert_eq!(run(named_import).len(), 1);
    }

    #[test]
    fn resolves_sleep_helpers_through_symbols() {
        let sleeping_helpers = r"
const pause = (ms) => new Promise(r => setTimeout(r, ms));
function nap(ms) { return new Promise(r => setTimeout(r, ms)); }
it('waits', async () => {
  await pause(100);
  await nap(100);
});
";
        assert_eq!(run(sleeping_helpers).len(), 2);
        let other_helpers = r"
import { setTimeout } from './clock';
const pause = (ms) => Promise.resolve(ms);
it('waits', async () => {
  await pause(100);
  await setTimeout(100);
});
";
        assert_eq!(run(other_helpers).len(), 0);
    }

    #[test]
    fn flags_sleep_in_run_now_callbacks() {
        let src = r"
it('waits', async () => {
  await act(async () => { await Bun.sleep(100); });
  p.then(async () => { await Bun.sleep(100); });
});
";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn ignores_sleep_in_handed_off_callback() {
        let src = r"
import { setTimeout as delay } from 'node:timers/promises';
it('serves', async () => {
  http.get('/x', async () => { await delay(200); });
  fn.mockImplementation(async () => { await Bun.sleep(50); });
});
";
        assert_eq!(run(src).len(), 0);
    }

    #[test]
    fn ignores_zero_delay() {
        let src = r"
const pause = (ms) => new Promise(r => setTimeout(r, ms));
it('flushes', async () => {
  await new Promise(r => setTimeout(r, 0));
  await new Promise(r => setTimeout(r));
  await pause(0);
});
";
        assert_eq!(run(src).len(), 0);
    }

    #[test]
    fn exempts_only_poll_loops() {
        let poll_loops = r"
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
it('polls', async () => {
  while (!ready()) await sleep(50);
  for (;;) { if (done()) break; await sleep(50); }
  for (let i = 0; i < 50 && !(await ready()); i++) await sleep(100);
});
";
        assert_eq!(run(poll_loops).len(), 0);
        let fixed_loop = r"
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
it('steps', async () => {
  for (const s of steps) { await sleep(100); }
});
";
        assert_eq!(run(fixed_loop).len(), 1);
    }

    #[test]
    fn ignores_promise_race_pending_probe() {
        let src = r"
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
it('is pending', async () => {
  await Promise.race([p, sleep(10)]);
});
";
        assert_eq!(run(src).len(), 0);
    }

    #[test]
    fn ignores_file_with_fake_timers() {
        let src = r"
beforeEach(() => { vi.useFakeTimers(); });
it('waits', async () => {
  await new Promise(r => setTimeout(r, 500));
});
";
        assert_eq!(run(src).len(), 0);
    }
}
