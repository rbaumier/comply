//! Shared heuristics for the test-assertion rules (`vitest-expect-expect`,
//! `assertions-in-tests`).

use crate::rules::backend::AstKind;
use oxc_ast::ast::Expression;
use oxc_semantic::NodeId;
use std::collections::HashSet;

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
