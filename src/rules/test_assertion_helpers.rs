//! Shared heuristics for the test-assertion rules (`vitest-expect-expect`,
//! `assertions-in-tests`).

use crate::rules::backend::AstKind;
use oxc_ast::ast::Expression;
use oxc_semantic::NodeId;
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
