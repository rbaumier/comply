use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["try"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else {
            return;
        };

        let body_span = try_stmt.block.span;
        if !body_has_unawaited_promise(semantic, body_span) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, body_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Promise inside try/catch without `await` \u{2014} rejection won't be caught."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True when the try block contains a promise-returning call that creates an
/// uncaught rejection: a floating `.then(onFulfilled)`, or a `fetch`/`axios`
/// call, that is neither `await`ed nor terminated by a chained `.catch()` that
/// handles the rejection.
fn body_has_unawaited_promise(
    semantic: &oxc_semantic::Semantic<'_>,
    body_span: oxc_span::Span,
) -> bool {
    semantic.nodes().iter().any(|n| {
        let AstKind::CallExpression(call) = n.kind() else {
            return false;
        };
        if !span_within(call.span, body_span) {
            return false;
        }
        if !is_unawaited_promise_call(call) {
            return false;
        }
        if is_chain_caught(semantic, n.id(), call.span) {
            return false;
        }
        !has_await_ancestor_within(semantic, n.id(), body_span)
    })
}

/// `inner` is fully contained in `outer`.
fn span_within(inner: oxc_span::Span, outer: oxc_span::Span) -> bool {
    inner.start >= outer.start && inner.end <= outer.end
}

/// Does the call create a promise whose rejection would escape the surrounding
/// `catch`?
///
/// - `x.then(onFulfilled)` with no rejection handler is a floating chain.
///   `x.then(onFulfilled, onRejected)` handles its own rejection, so it is not
///   flagged. A bare `x.then` property read is a member expression, not a call,
///   so it never reaches here.
/// - `fetch(...)` returns a promise.
/// - `axios(...)` / `axios.get|post|put|delete|patch(...)` return promises.
fn is_unawaited_promise_call(call: &CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => id.name.as_str() == "fetch",
        Expression::StaticMemberExpression(member) => {
            let method = member.property.name.as_str();
            if method == "then" {
                return call.arguments.len() < 2;
            }
            is_axios_method_call(member, method)
        }
        _ => false,
    }
}

/// `axios.get(...)` / `axios.post(...)` etc. on the `axios` identifier.
fn is_axios_method_call(member: &StaticMemberExpression, method: &str) -> bool {
    if !matches!(method, "get" | "post" | "put" | "delete" | "patch") {
        return false;
    }
    matches!(&member.object, Expression::Identifier(obj) if obj.name.as_str() == "axios")
}

/// True when the promise-returning call at `call_id` is the receiver of a
/// downstream `.catch()` in the same chain, so its rejection is handled and the
/// surrounding try/catch is irrelevant to that path.
///
/// The call is chain-caught iff it is the `object` (receiver) of a
/// `StaticMemberExpression` that is itself the `callee` of a `CallExpression`,
/// AND either that member's method is `catch`, or the member is a further
/// promise-chain link (`then`/`finally`) whose enclosing call is also
/// chain-caught. Any other method may return a non-promise, so it breaks the
/// chain and stops the walk. The receiver-position requirement at every step
/// keeps `foo(p.then(a)).catch(b)` flagged: there the `.then()` is an argument
/// to `foo`, so the `.catch()` handles `foo(...)`'s result, a different promise.
fn is_chain_caught(
    semantic: &oxc_semantic::Semantic<'_>,
    call_id: oxc_semantic::NodeId,
    call_span: oxc_span::Span,
) -> bool {
    use oxc_span::GetSpan;
    let nodes = semantic.nodes();

    let member_id = nodes.parent_id(call_id);
    if member_id == call_id {
        return false;
    }
    let AstKind::StaticMemberExpression(member) = nodes.get_node(member_id).kind() else {
        return false;
    };
    // The call must be the receiver (object) of the member access, not an
    // argument nested somewhere inside it.
    if member.object.span() != call_span {
        return false;
    }

    let outer_id = nodes.parent_id(member_id);
    if outer_id == member_id {
        return false;
    }
    let AstKind::CallExpression(outer) = nodes.get_node(outer_id).kind() else {
        return false;
    };
    // The member must be the callee of the enclosing call (`<chain>.method(...)`),
    // not one of its arguments.
    if outer.callee.span() != member.span {
        return false;
    }

    match member.property.name.as_str() {
        "catch" => true,
        "then" | "finally" => is_chain_caught(semantic, outer_id, outer.span),
        _ => false,
    }
}

/// True when an `await` lies between this node and the try block, i.e. the call
/// is awaited within the try body.
fn has_await_ancestor_within(
    semantic: &oxc_semantic::Semantic<'_>,
    node_id: oxc_semantic::NodeId,
    body_span: oxc_span::Span,
) -> bool {
    semantic.nodes().ancestors(node_id).any(|ancestor| {
        matches!(ancestor.kind(), AstKind::AwaitExpression(aw) if span_within(aw.span, body_span))
    })
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
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_fetch_without_await_in_try() {
        let d = run("try {\n  const res = fetch(\"/api\");\n} catch (e) {}\n");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-try-promise");
    }

    #[test]
    fn flags_floating_then_without_await_in_try() {
        let d = run("try {\n  getData().then(r => r.json());\n} catch (e) {}\n");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_awaited_fetch_in_try() {
        let d = run("try {\n  const res = await fetch(\"/api\");\n} catch (e) {}\n");
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_fetch_outside_try() {
        assert!(run("const res = fetch(\"/api\");").is_empty());
    }

    // Regression for issue #1724: `typeof x.then === 'function'` is a property
    // existence check (a member read, not a call) and a two-argument
    // `thenable.then(onFulfilled, onRejected)` handles its own rejection.
    // Neither creates an uncaught rejection, so the try block must not be flagged.
    #[test]
    fn allows_thenable_guard_and_two_arg_then() {
        let src = "\
function withGlobalActEnvironment(actImplementation) {
  return callback => {
    try {
      let callbackNeedsToBeAwaited = false
      const actResult = actImplementation(() => {
        const result = callback()
        if (
          result !== null &&
          typeof result === 'object' &&
          typeof result.then === 'function'
        ) {
          callbackNeedsToBeAwaited = true
        }
        return result
      })
      if (callbackNeedsToBeAwaited) {
        const thenable = actResult
        return {
          then: (resolve, reject) => {
            thenable.then(
              returnValue => { resolve(returnValue) },
              error => { reject(error) },
            )
          },
        }
      }
    } catch (error) {
      throw error
    }
  }
}
";
        assert!(
            run(src).is_empty(),
            "thenable guard + two-arg .then(fulfil, reject) must not be flagged"
        );
    }

    #[test]
    fn allows_typeof_then_property_check_alone() {
        let src = "\
try {
  if (typeof result.then === 'function') {
    doSomething();
  }
} catch (e) {}
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_two_arg_then_in_try() {
        let src = "\
try {
  promise.then(onFulfilled, onRejected);
} catch (e) {}
";
        assert!(run(src).is_empty());
    }

    // Negative-space guard: a genuine single-argument floating `.then(...)` chain
    // inside a try block — the real pattern the rule targets — stays flagged.
    #[test]
    fn still_flags_single_arg_then_in_try() {
        let d = run("try {\n  promise.then(onFulfilled);\n} catch (e) {}\n");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_axios_get_without_await_in_try() {
        let d = run("try {\n  axios.get(\"/api\");\n} catch (e) {}\n");
        assert_eq!(d.len(), 1);
    }

    // Regression for issue #7057: a single-arg `.then(onFulfilled)` whose result
    // is immediately caught by a chained `.catch(onRejected)` handles its own
    // rejection — the try/catch is irrelevant to that path, so it must not be
    // flagged.
    #[test]
    fn allows_then_chained_with_catch_in_try() {
        let src = "\
try {
  return Promise.resolve(handler(req, res))
    .then(() => resolve(kHandled))
    .catch((error) => reject(error));
} catch (error) {
  reject(error);
}
";
        assert!(
            run(src).is_empty(),
            ".then(fulfil).catch(reject) chain handles its own rejection"
        );
    }

    // A `.finally()` interposed between `.then()` and `.catch()` still leaves the
    // rejection handled by the trailing `.catch()`.
    #[test]
    fn allows_then_finally_catch_chain_in_try() {
        let src = "\
try {
  promise.then((v) => use(v)).finally(cleanup).catch((e) => log(e));
} catch (e) {}
";
        assert!(run(src).is_empty());
    }

    // Receiver-position guard: here the `.then()` is an *argument* to `foo`, and
    // the `.catch()` handles `foo(...)`'s result, not the `.then()` result — the
    // `.then()` rejection can still escape, so the try block stays flagged.
    #[test]
    fn still_flags_then_as_argument_with_outer_catch() {
        let d = run(
            "try {\n  foo(promise.then((v) => use(v))).catch((e) => log(e));\n} catch (e) {}\n",
        );
        assert_eq!(d.len(), 1);
    }
}
