//! Shared heuristics for the test-assertion rules (`vitest-expect-expect`,
//! `assertions-in-tests`).

use crate::rules::backend::AstKind;
use oxc_ast::ast::{CallExpression, Expression};
use oxc_semantic::NodeId;
use oxc_span::GetSpan;
use std::collections::HashSet;

/// React render functions that throw when the component/hook crashes. A test
/// whose body is only `render(<App/>)`, `renderToString(<C/>)`, or
/// `renderHook(() => useForm())` is a "does not throw" smoke test: reaching the
/// end means rendering succeeded, so the render call *is* the assertion. React
/// Testing Library and `react-dom/server` document these as valid no-throw
/// tests, so a body relying solely on them is not assertion-less.
pub(crate) const RENDER_ASSERTION_CALLS: &[&str] =
    &["render", "renderToString", "renderToStaticMarkup", "renderHook"];

/// True when `text` contains a call to one of [`RENDER_ASSERTION_CALLS`]. The
/// identifier is word-boundary-anchored on the left so `customRender(` /
/// `prerenderToString(` do not match, and the call's `(` must immediately
/// follow the name.
pub(crate) fn has_render_assertion_call(text: &str) -> bool {
    let bytes = text.as_bytes();
    for name in RENDER_ASSERTION_CALLS {
        let mut from = 0usize;
        while let Some(rel) = text[from..].find(name) {
            let i = from + rel;
            let prev_ok = i == 0
                || !(bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'$');
            let after = i + name.len();
            if prev_ok && bytes.get(after) == Some(&b'(') {
                return true;
            }
            from = i + name.len();
        }
    }
    false
}

/// True when a `throw` statement lies within `body_span`. A `throw` is a valid
/// assertion mechanism: timing/property/fuzzing tests fail by throwing on a
/// violated condition (`if (after - before > 10) throw new Error(...)`), which
/// the test runner reports as a failure — functionally equivalent to an
/// `expect(...)` call. A test whose body throws is therefore not assertion-less.
pub(crate) fn body_contains_throw(
    semantic: &oxc_semantic::Semantic<'_>,
    body_span: oxc_span::Span,
) -> bool {
    semantic.nodes().iter().any(|n| {
        if let AstKind::ThrowStatement(throw) = n.kind() {
            throw.span.start >= body_span.start && throw.span.end <= body_span.end
        } else {
            false
        }
    })
}

/// True when the test callback (2nd argument of an `it`/`test` call) invokes an
/// identifier bound to a formal parameter of an *enclosing* function. Such a
/// test is a factory whose real body — and its assertions — is supplied by the
/// wrapper's callers:
///
/// ```ts
/// function txIt(name: string, fn: () => Promise<void>): void {
///   it(name, async () => { await fn(); }); // assertions live inside `fn`
/// }
/// ```
///
/// The inline `it` body having no `expect(...)` is therefore not a missing
/// assertion. The test callback's own parameters do not count — only those of
/// functions that lexically enclose the `it`/`test` call.
pub(crate) fn delegates_to_outer_param(
    it_call: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();

    // Functions enclosing the it() call — i.e. wrappers OUTSIDE the test
    // callback (the callback is a child of the call, never an ancestor).
    let outer_fns: HashSet<NodeId> = nodes
        .ancestors(it_call.id())
        .filter(|a| {
            matches!(
                a.kind(),
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
            )
        })
        .map(|a| a.id())
        .collect();
    if outer_fns.is_empty() {
        return false;
    }

    let AstKind::CallExpression(call) = it_call.kind() else {
        return false;
    };
    let (lo, hi) = (call.span.start, call.span.end);

    for node in nodes.iter() {
        let AstKind::CallExpression(inner) = node.kind() else {
            continue;
        };
        if inner.span.start < lo || inner.span.end > hi {
            continue;
        }
        let Expression::Identifier(callee) = &inner.callee else {
            continue;
        };
        let Some(ref_id) = callee.reference_id.get() else {
            continue;
        };
        let Some(sym_id) = semantic.scoping().get_reference(ref_id).symbol_id() else {
            continue;
        };
        let decl = semantic.scoping().symbol_declaration(sym_id);
        if binding_is_outer_param(decl, &outer_fns, semantic) {
            return true;
        }
    }
    false
}

/// True when `call`'s callee is a bare identifier bound to the first formal
/// parameter (the `resolve` slot) of an enclosing `new Promise((resolve) => …)`
/// executor. In a promise-returning test, reaching that resolve call is the
/// implicit assertion: if it is never invoked the promise never settles and the
/// runner fails the test by timeout. A body whose only completion path calls
/// the resolve parameter is therefore not assertion-less:
///
/// ```ts
/// test("queue a task", () =>
///   new Promise(done => { requestCallback(() => { done(undefined); }); }));
/// ```
///
/// Unlike [`is_promise_reject_assertion`], no `new Error(...)` argument is
/// required — `done()` / `resolve(value)` all count.
pub(crate) fn is_promise_resolve_call(
    call: &CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    call_callee_is_promise_executor_param(call, semantic, 0)
}

/// True when `call` is `reject(new Error(...))` (or any `*Error` constructor)
/// where `reject` is bound to the second formal parameter of an enclosing
/// `new Promise((resolve, reject) => …)` executor. Reaching this rejection
/// fails the test with that error, so it *is* the assertion.
pub(crate) fn is_promise_reject_assertion(
    call: &CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    // First argument must be `new Error(...)` (or any `*Error` constructor).
    let Some(oxc_ast::ast::Argument::NewExpression(new_expr)) = call.arguments.first() else {
        return false;
    };
    let Expression::Identifier(ctor) = &new_expr.callee else {
        return false;
    };
    if !ctor.name.ends_with("Error") {
        return false;
    }
    call_callee_is_promise_executor_param(call, semantic, 1)
}

/// True when a call within `body_span` invokes the resolve parameter of an
/// enclosing `new Promise(...)` executor — see [`is_promise_resolve_call`].
pub(crate) fn body_contains_promise_resolve_call(
    semantic: &oxc_semantic::Semantic<'_>,
    body_span: oxc_span::Span,
) -> bool {
    semantic.nodes().iter().any(|n| {
        let AstKind::CallExpression(call) = n.kind() else {
            return false;
        };
        call.span.start >= body_span.start
            && call.span.end <= body_span.end
            && is_promise_resolve_call(call, semantic)
    })
}

/// True when `call`'s callee is a bare identifier bound to the formal parameter
/// at `param_index` of an enclosing `new Promise(...)` executor.
fn call_callee_is_promise_executor_param(
    call: &CallExpression,
    semantic: &oxc_semantic::Semantic,
    param_index: usize,
) -> bool {
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    let Some(ref_id) = callee.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl = scoping.symbol_declaration(sym_id);
    declaration_is_promise_executor_param(decl, semantic, param_index)
}

/// True when `decl` is the formal parameter at `param_index` of a function
/// passed as the executor to `new Promise(...)`.
fn declaration_is_promise_executor_param(
    decl: NodeId,
    semantic: &oxc_semantic::Semantic,
    param_index: usize,
) -> bool {
    let nodes = semantic.nodes();

    // Find the enclosing function and the binding's span.
    let decl_span = nodes.kind(decl).span();
    let executor_id = std::iter::once(nodes.get_node(decl))
        .chain(nodes.ancestors(decl))
        .find(|anc| {
            matches!(
                anc.kind(),
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
            )
        })
        .map(|anc| anc.id());
    let Some(executor_id) = executor_id else {
        return false;
    };

    // The executor's parent must be `new Promise(...)`.
    let parent_id = nodes.parent_id(executor_id);
    let AstKind::NewExpression(new_expr) = nodes.kind(parent_id) else {
        return false;
    };
    let Expression::Identifier(ctor) = &new_expr.callee else {
        return false;
    };
    if ctor.name.as_str() != "Promise" {
        return false;
    }

    // The binding must be the formal parameter at `param_index`.
    let params = match nodes.kind(executor_id) {
        AstKind::Function(f) => &f.params,
        AstKind::ArrowFunctionExpression(f) => &f.params,
        _ => return false,
    };
    params.items.get(param_index).is_some_and(|param| {
        param.span.start <= decl_span.start && decl_span.end <= param.span.end
    })
}

/// True when `decl` is a formal-parameter binding of one of `outer_fns`.
fn binding_is_outer_param(
    decl: NodeId,
    outer_fns: &HashSet<NodeId>,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut saw_param = matches!(
        nodes.kind(decl),
        AstKind::FormalParameter(_) | AstKind::FormalParameters(_)
    );
    for anc in nodes.ancestors(decl) {
        match anc.kind() {
            AstKind::FormalParameter(_) | AstKind::FormalParameters(_) => saw_param = true,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return saw_param && outer_fns.contains(&anc.id());
            }
            _ => {}
        }
    }
    false
}
