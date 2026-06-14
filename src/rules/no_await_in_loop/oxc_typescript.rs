//! no-await-in-loop OXC backend — flag `await` inside a loop body, but
//! exempt recursive calls to the enclosing async function (deliberate
//! depth-first traversal) and retry/polling loops that exit early on a
//! result and pace themselves with a delay/backoff `await`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, PropertyKey, Statement};
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

pub struct Check;

/// Extract the identifier name of the call target, if the awaited
/// expression is a direct call to an identifier or a `this.method` call.
/// `obj.method()` is NOT treated as self-recursion — only `this.method()` is.
fn awaited_callee_name<'a>(arg: &Expression<'a>) -> Option<&'a str> {
    let Expression::CallExpression(call) = arg else {
        return None;
    };
    match &call.callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            if matches!(member.object, Expression::ThisExpression(_)) {
                Some(member.property.name.as_str())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Whether the awaited expression is a `Promise` combinator call that
/// coordinates multiple promises in parallel: `Promise.all`,
/// `Promise.allSettled`, `Promise.race`, or `Promise.any`. Awaiting one
/// of these inside a loop is a deliberate batching pattern (the items are
/// already parallelized), not the serial-await anti-pattern.
fn is_awaited_promise_combinator(arg: &Expression) -> bool {
    let Expression::CallExpression(call) = arg else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let Expression::Identifier(object) = &member.object else {
        return false;
    };
    object.name == "Promise"
        && matches!(
            member.property.name.as_str(),
            "all" | "allSettled" | "race" | "any"
        )
}

/// Whether `await_span` falls within the once-evaluated header of a loop
/// rather than its per-iteration body. The iterable of a `for-of`/`for-in`
/// (`right`) and the `init` clause of a C-style `for(;;)` run exactly once
/// before iteration, so an `await` there is not a serial-per-iteration await.
/// The `test`/`update` of a `for(;;)` and every loop body DO run per
/// iteration, so awaits there still count.
fn await_in_loop_header(loop_kind: &AstKind, await_span: Span) -> bool {
    let contains = |span: Span| span.start <= await_span.start && await_span.end <= span.end;
    match loop_kind {
        AstKind::ForOfStatement(stmt) => contains(stmt.right.span()),
        AstKind::ForInStatement(stmt) => contains(stmt.right.span()),
        AstKind::ForStatement(stmt) => {
            stmt.init.as_ref().is_some_and(|init| contains(init.span()))
        }
        _ => false,
    }
}

/// Whether an awaited call targets a delay/backoff primitive — `delay`,
/// `sleep`, `wait`, `setTimeout`, or `setInterval`, called bare or as a
/// member (`timers.setTimeout`, `this.sleep`). An `await` on one of these
/// is sequential pacing (polling/backoff), not parallelizable work.
fn is_delay_await(arg: &Expression) -> bool {
    let Expression::CallExpression(call) = arg else {
        return false;
    };
    let name = match &call.callee {
        Expression::Identifier(id) => id.name.as_str(),
        Expression::StaticMemberExpression(member) => member.property.name.as_str(),
        _ => return false,
    };
    matches!(name, "delay" | "sleep" | "wait" | "setTimeout" | "setInterval")
}

/// Whether an expression statement is `await <delay-call>`.
fn stmt_is_delay_await(stmt: &Statement) -> bool {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return false;
    };
    let Expression::AwaitExpression(await_expr) = &expr_stmt.expression else {
        return false;
    };
    is_delay_await(&await_expr.argument)
}

/// Recursively scan the statements of a loop body, recording whether it
/// contains an early exit (`return`/`break`) and a delay/backoff `await`.
/// Nested loops and nested functions are not descended into — their
/// statements belong to a different iteration context.
fn scan_retry_signals(stmt: &Statement, has_exit: &mut bool, has_delay: &mut bool) {
    if stmt_is_delay_await(stmt) {
        *has_delay = true;
    }
    match stmt {
        Statement::ReturnStatement(_) | Statement::BreakStatement(_) => {
            *has_exit = true;
        }
        Statement::BlockStatement(block) => {
            for s in &block.body {
                scan_retry_signals(s, has_exit, has_delay);
            }
        }
        Statement::IfStatement(if_stmt) => {
            scan_retry_signals(&if_stmt.consequent, has_exit, has_delay);
            if let Some(alt) = &if_stmt.alternate {
                scan_retry_signals(alt, has_exit, has_delay);
            }
        }
        Statement::TryStatement(t) => {
            for s in &t.block.body {
                scan_retry_signals(s, has_exit, has_delay);
            }
            if let Some(h) = &t.handler {
                for s in &h.body.body {
                    scan_retry_signals(s, has_exit, has_delay);
                }
            }
            if let Some(f) = &t.finalizer {
                for s in &f.body {
                    scan_retry_signals(s, has_exit, has_delay);
                }
            }
        }
        Statement::LabeledStatement(l) => scan_retry_signals(&l.body, has_exit, has_delay),
        Statement::SwitchStatement(sw) => {
            for case in &sw.cases {
                for s in &case.consequent {
                    scan_retry_signals(s, has_exit, has_delay);
                }
            }
        }
        // Nested loops / functions start a fresh iteration or async context;
        // their bodies are not part of this loop's per-iteration shape.
        _ => {}
    }
}

/// Whether a loop is a retry/polling loop that is sequential by design:
/// its body both exits early on a result (`return`/`break`) and paces
/// itself with a delay/backoff `await`. Such a loop cannot be rewritten as
/// `Promise.all` — each iteration depends on the previous attempt's outcome.
fn is_retry_polling_loop(loop_kind: &AstKind) -> bool {
    let body = match loop_kind {
        AstKind::ForStatement(stmt) => &stmt.body,
        AstKind::ForOfStatement(stmt) => &stmt.body,
        AstKind::ForInStatement(stmt) => &stmt.body,
        AstKind::WhileStatement(stmt) => &stmt.body,
        AstKind::DoWhileStatement(stmt) => &stmt.body,
        _ => return false,
    };
    let mut has_exit = false;
    let mut has_delay = false;
    scan_retry_signals(body, &mut has_exit, &mut has_delay);
    has_exit && has_delay
}

/// Walk ancestors of the `await` looking for a loop boundary. Stops at
/// function boundaries (a nested `async` function starts a fresh
/// context — its awaits are not "in" the outer loop). Returns the name
/// of the enclosing async function when a loop is found, so the caller
/// can compare against the awaited callee for recursion detection.
///
/// An `await` in a loop's once-evaluated header (the `for-of`/`for-in`
/// iterable, or a `for(;;)` `init`) is not treated as in-loop.
///
/// Return values:
///   - `Some(Some(name))` — inside a loop in a named async function
///   - `Some(None)` — inside a loop in an unnamed/arrow async function
///   - `None` — not inside a loop (or the enclosing function is reached first)
fn enclosing_loop_and_fn_name<'a>(
    node_id: oxc_semantic::NodeId,
    await_span: Span,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<Option<&'a str>> {
    let nodes = semantic.nodes();
    let mut current_id = node_id;
    let mut saw_loop = false;
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return None;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            kind @ (AstKind::ForStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_)) => {
                if !await_in_loop_header(&kind, await_span) {
                    // Retry/polling exemption: the nearest enclosing loop
                    // exits early on a result and paces itself with a delay —
                    // it is sequential by design and cannot be parallelized.
                    if !saw_loop && is_retry_polling_loop(&kind) {
                        return None;
                    }
                    saw_loop = true;
                }
            }
            AstKind::Function(func) => {
                if !saw_loop {
                    return None;
                }
                // Named function declarations/expressions have their own id.
                if let Some(id) = &func.id {
                    return Some(Some(id.name.as_str()));
                }
                // Class methods (`async method() {}`) have no func.id — the
                // name lives on the parent MethodDefinition's key.
                let gp_id = nodes.parent_id(parent_id);
                if gp_id != parent_id {
                    if let AstKind::MethodDefinition(method) = nodes.get_node(gp_id).kind() {
                        if let PropertyKey::StaticIdentifier(id) = &method.key {
                            return Some(Some(id.name.as_str()));
                        }
                    }
                }
                return Some(None);
            }
            AstKind::ArrowFunctionExpression(_) => {
                if !saw_loop {
                    return None;
                }
                // Arrow functions are nameless at the syntax level. Try to
                // recover the conventional name from the parent binding.
                let gp_id = nodes.parent_id(parent_id);
                if gp_id != parent_id {
                    let gp_kind = nodes.get_node(gp_id).kind();
                    // `const foo = async () => {}` — VariableDeclarator binding.
                    if let AstKind::VariableDeclarator(decl) = gp_kind
                        && let BindingPattern::BindingIdentifier(id) = &decl.id
                    {
                        return Some(Some(id.name.as_str()));
                    }
                    // `foo = async () => {}` as a class property — PropertyDefinition key.
                    if let AstKind::PropertyDefinition(prop) = gp_kind
                        && let PropertyKey::StaticIdentifier(id) = &prop.key
                    {
                        return Some(Some(id.name.as_str()));
                    }
                }
                return Some(None);
            }
            _ => {}
        }
        current_id = parent_id;
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AwaitExpression(await_expr) = node.kind() else {
            return;
        };

        let Some(enclosing_fn_name) =
            enclosing_loop_and_fn_name(node.id(), await_expr.span, semantic)
        else {
            return;
        };

        // Batching exception: `await Promise.all/allSettled/race/any(...)`
        // already coordinates multiple promises in parallel. The outer
        // sequential loop is intentional (back-pressure, rate limiting,
        // data-dependency between batches), not serial-await-per-iteration.
        if is_awaited_promise_combinator(&await_expr.argument) {
            return;
        }

        // Recursion exception: if the awaited expression is a direct
        // call to the enclosing async function, skip — sequential
        // recursion is the only way to express depth-first traversal.
        if let (Some(fn_name), Some(callee)) =
            (enclosing_fn_name, awaited_callee_name(&await_expr.argument))
            && fn_name == callee
        {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Sequential `await` in a loop serializes independent work. \
                      If the iterations don't depend on each other, use \
                      `Promise.all(items.map(f))` instead."
                .into(),
            severity: Severity::Error,
            span: None,
        });
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
