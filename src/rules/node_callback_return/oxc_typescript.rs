//! node-callback-return OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement};
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

pub struct Check;

const CALLBACKS: &[&str] = &["callback", "cb", "next"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["callback", "cb", "next"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if !CALLBACKS.contains(&callee.name.as_str()) {
            return;
        }

        // A zero-argument call carries no error and no result, so there is
        // nothing for a missing `return` to drop. `next()`/`cb()` with no
        // arguments is a side-effecting continuation/notification, not a Node
        // error-first callback (`cb(err)`, `callback(err, data)`), so there's
        // no propagation hazard.
        if call.arguments.is_empty() {
            return;
        }

        // If the call sits inside an arrow function whose body is an
        // expression (e.g. `(x) => cb(x)` or `(x) => wrap(cb(x))`), the
        // value is implicitly returned — there's no "forgot return" risk.
        if inside_implicit_return_arrow(node, semantic) {
            return;
        }

        // Walk up to find the parent statement context.
        let parent = semantic.nodes().parent_node(node.id());
        match parent.kind() {
            // `return cb(err);`
            AstKind::ReturnStatement(_) => return,
            // Arrow body: `(err) => cb(err)`
            AstKind::ArrowFunctionExpression(_) => return,
            // `await callback(...)` — the call is awaited, so execution continues
            // afterwards by design (capture result, post-downstream cleanup, or the
            // Koa/Hono/Fastify "wrap" middleware pattern `await next(); <post-processing>`).
            // An awaited call is structurally not a fire-and-forget Node error-first
            // callback, so a trailing `return` is neither expected nor correct.
            AstKind::AwaitExpression(_) => return,
            // `push(callback(key))` / `new Wrapper(cb(x))` — the call's result is
            // passed as an argument to an enclosing call, so it is consumed
            // downstream, not dropped. A trailing `return` is impossible (the value
            // must flow into the outer call). Only exempt the arguments position:
            // for `callback(x)(y)` the inner call is the outer call's callee, whose
            // span matches no argument, so it stays flagged.
            AstKind::CallExpression(outer) if is_argument(call.span, &outer.arguments) => {
                return;
            }
            AstKind::NewExpression(outer) if is_argument(call.span, &outer.arguments) => {
                return;
            }
            AstKind::ExpressionStatement(expr_stmt) => {
                let grandparent = semantic.nodes().parent_node(parent.id());
                if let AstKind::FunctionBody(block) = grandparent.kind() {
                    let stmts = &block.statements;
                    // Find our position in the block.
                    let our_span = expr_stmt.span;
                    let mut found_idx = None;
                    for (i, s) in stmts.iter().enumerate() {
                        if s.span() == our_span {
                            found_idx = Some(i);
                            break;
                        }
                    }
                    if let Some(idx) = found_idx {
                        // Check if the next statement is a return or throw.
                        if let Some(next) = stmts.get(idx + 1)
                            && matches!(
                                next,
                                Statement::ReturnStatement(_) | Statement::ThrowStatement(_)
                            ) {
                                return;
                            }

                        // If this is the last statement in a function body, it's
                        // fine — unless the enclosing function is itself a
                        // callback argument, in which case the rule still applies.
                        if idx == stmts.len() - 1 {
                            let great_grandparent =
                                semantic.nodes().parent_node(grandparent.id());
                            match great_grandparent.kind() {
                                AstKind::Function(_) => return,
                                AstKind::ArrowFunctionExpression(_) => {
                                    // Only exempt if the arrow is not itself
                                    // passed as an argument to another call.
                                    let ggp_parent = semantic
                                        .nodes()
                                        .parent_node(great_grandparent.id());
                                    if !matches!(
                                        ggp_parent.kind(),
                                        AstKind::CallExpression(_)
                                            | AstKind::StaticMemberExpression(_)
                                            | AstKind::ComputedMemberExpression(_)
                                    ) {
                                        return;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Expected `return` with your callback function.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Return true if `call_span` matches one of the enclosing call's arguments,
/// i.e. the flagged call sits in arguments position rather than being the
/// callee. Spread arguments do not reach here (their semantic parent is the
/// `SpreadElement`, not the call), so a direct span match is exact.
fn is_argument(call_span: Span, arguments: &[Argument<'_>]) -> bool {
    arguments.iter().any(|arg| arg.span() == call_span)
}

/// Walk up from `node`; return true only if we reach an
/// `ArrowFunctionExpression` with `expression: true` (implicit-return arrow)
/// without crossing any scope boundary.
///
/// OXC's semantic parent chain can skip intermediate AST nodes: for
/// `(x) => outer((y) => { cb(y); })` the inner `ArrowFunctionExpression`
/// (expression: false) does not always appear as a semantic parent. Its
/// `FunctionBody` ends up with the outer arrow as its semantic parent.
/// We handle this by resolving `FunctionBody` eagerly: when we see a
/// `FunctionBody` node we immediately check its parent. If that parent is
/// `ArrowFunctionExpression(expression: false)` we stop (block-body scope).
/// If it is `ArrowFunctionExpression(expression: true)` but the current
/// `FunctionBody` does not match the arrow's own body span, we know a
/// block-body arrow was elided and stop. Otherwise we continue.
fn inside_implicit_return_arrow<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut cur = node;
    loop {
        let p = semantic.nodes().parent_node(cur.id());
        if p.id() == cur.id() {
            return false;
        }
        match p.kind() {
            AstKind::ArrowFunctionExpression(arrow) => return arrow.expression,
            // FunctionBody appears both as the synthetic wrapper for an
            // expression-body arrow and as the real block body. Resolve it
            // eagerly by peeking at its own parent.
            AstKind::FunctionBody(fb) => {
                let pp = semantic.nodes().parent_node(p.id());
                if pp.id() == p.id() {
                    return false;
                }
                match pp.kind() {
                    AstKind::ArrowFunctionExpression(arrow) => {
                        if !arrow.expression {
                            // Real block-body arrow owns this FunctionBody.
                            return false;
                        }
                        // Expression-body arrow: verify this FunctionBody is
                        // the arrow's own synthetic body and not one from a
                        // skipped inner block-body arrow.
                        if arrow.body.span() != fb.span() {
                            return false;
                        }
                        // The FunctionBody belongs to this implicit-return arrow.
                        return true;
                    }
                    AstKind::Function(_) => return false,
                    _ => return false,
                }
            }
            // ExpressionStatement wraps the synthetic statement OXC creates
            // around an arrow's expression body — transparent.
            AstKind::ExpressionStatement(_) => {}
            // Any non-arrow function, block, or statement — stop.
            AstKind::Function(_)
            | AstKind::Program(_)
            | AstKind::BlockStatement(_)
            | AstKind::ReturnStatement(_)
            | AstKind::IfStatement(_)
            | AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_)
            | AstKind::SwitchStatement(_)
            | AstKind::TryStatement(_)
            | AstKind::ThrowStatement(_)
            | AstKind::LabeledStatement(_)
            | AstKind::VariableDeclaration(_) => return false,
            // Expression wrappers — transparent.
            AstKind::CallExpression(_)
            | AstKind::StaticMemberExpression(_)
            | AstKind::ComputedMemberExpression(_)
            | AstKind::SequenceExpression(_)
            | AstKind::ParenthesizedExpression(_)
            | AstKind::ConditionalExpression(_)
            | AstKind::BinaryExpression(_)
            | AstKind::LogicalExpression(_)
            | AstKind::UnaryExpression(_)
            | AstKind::TSAsExpression(_)
            | AstKind::TSTypeAssertion(_)
            | AstKind::TSNonNullExpression(_)
            | AstKind::TSSatisfiesExpression(_)
            | AstKind::ArrayExpression(_)
            | AstKind::ObjectExpression(_)
            | AstKind::ObjectProperty(_) => {}
            // Default: stop. Anything not in the transparent list could
            // introduce a new scope or statement boundary.
            _ => return false,
        }
        cur = p;
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_cb_without_return() {
        let src = "function handle(err) { if (err) { cb(err); } doMore(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_return_cb() {
        let src = "function handle(err) { if (err) { return cb(err); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_cb_as_last_in_function() {
        let src = "function handle(err) { cb(err); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_arrow_implicit_return_callback() {
        // Issue #157: arrow with expression body — `next(search)` is implicitly returned.
        let src = r#"
            const middlewares = [
                ({ search, next }) => stripDefaults(next(search), defaults),
            ];
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_arrow_implicit_return_direct_callback() {
        let src = "const fn = (err) => cb(err);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_call_in_block_arrow_inside_implicit_arrow() {
        // Regression: cb inside a block-body arrow that is itself an argument
        // to a call inside an implicit-return arrow must still be flagged.
        let src = "const outer = (x) => inner((y) => { cb(y); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_when_callback_result_is_awaited_and_captured() {
        // Regression #547: `const result = await callback(conn)` followed by cleanup
        // before `return result` must not be flagged.
        let src = r#"
            async function wrap(callback) {
              try {
                const result = await callback(conn);
                await conn.unsafe("RELEASE SAVEPOINT sp");
                return result;
              } catch (err) {
                await conn.unsafe("ROLLBACK TO SAVEPOINT sp");
                throw err;
              }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_return_await_callback() {
        // `return await callback(...)` — explicitly returned, not a Node FP.
        let src = "async function wrap(callback) { return await callback(conn); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_awaited_next_then_post_processing() {
        // Issue #1220: Koa/Hono/Fastify "wrap" middleware awaits the downstream
        // chain then post-processes the response. `await next()` followed by more
        // statements is intentional — `return next()` would skip the post-processing.
        let src = r#"
            app.use('/favicon-notfound.ico', async (c, next) => {
              await next()
              c.header('X-Custom', 'Deno')
            })
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_fire_and_forget_callback() {
        // Negative space: a genuine non-awaited, non-returned Node error-first
        // callback followed by more work is still flagged.
        let src = "function f(cb) { cb(err); doMore(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_on_zero_arg_next_continuation() {
        // Issue #3968: a Zimmerframe visitor continuation `next()` is called for
        // its side effect with zero arguments, then the function does dependent
        // work. A no-arg call propagates nothing, so a missing `return` is fine.
        let src = "function v(node, { next }) { next(); const x = []; doWork(x); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_zero_arg_cb_then_work() {
        // A bare `cb();` (no arguments) followed by more work carries no
        // error/result, so there is no propagation hazard.
        let src = "function f(cb) { cb(); doMore(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_callback_with_err_and_data() {
        let src = "function f(callback) { callback(err, data); doMore(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_next_with_error() {
        let src = "function f(next) { next(error); doMore(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_when_callback_result_pushed_into_array() {
        // Issue #3958: `values.push(callback(key))` — the callback's result is
        // passed as an argument to `push`, so it is consumed, not dropped. A
        // trailing `return` is impossible (the value must be pushed and iteration
        // must continue).
        let src = "objectForEachKey(obj, key => { values.push(callback(key)); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_callback_result_is_call_argument() {
        // The callback's result flows as an argument into an enclosing call.
        let src = "function f(cb) { arr.push(cb(x)); doMore(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_callback_result_wrapped_in_call() {
        let src = "function f(next) { wrap(next(y)); doMore(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_callback_result_is_new_argument() {
        let src = "function f(callback) { const e = new Error(callback(x)); throw e; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_callback_as_callee_of_outer_call() {
        // `cb(x)(y)` — the inner `cb(x)` is the OUTER call's callee, not an
        // argument; its result is dropped, so it stays flagged.
        let src = "function f(cb) { cb(err)(y); doMore(); }";
        assert_eq!(run(src).len(), 1);
    }
}
