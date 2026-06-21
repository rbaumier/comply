//! Shared detection for a `new X()` statement that is the subject of a
//! throw assertion in a test.
//!
//! A constructor invoked for its side effect normally signals a misused
//! constructor (`no-constructor-side-effects`, the delegated `no-new`). But
//! inside a throw-assertion callback the discard is the test's intent: the
//! assertion is precisely that *constructing* `X` throws, so the new instance is
//! meant to be thrown away. The recognized callback wrappers are:
//!
//!   1. `t.throws(cb, ...)` / `assert.throws(cb)` / `assert.rejects(cb)` — the
//!      callee is a member access or bare identifier named `throws` /
//!      `throwsAsync` / `rejects` and `cb` is its first argument.
//!   2. Jest: `expect(cb).toThrow()` / `.toThrowError()` — `expect(cb)` is the
//!      object of a `.toThrow*` member access.
//!   3. Chai: `expect(cb).to.throw()` / `.to.throws()` — `expect(cb)` is the
//!      receiver of a `.throw` / `.throws` member call reached through any chain
//!      of intermediate member accesses (`to`, `not`, `does`, `which`, …).

use crate::rules::backend::AstKind;
use oxc_ast::ast::Expression;

/// True when the `NewExpression` identified by `new_node_id` sits inside a
/// callback that is the subject of a throw assertion. Walks up to the nearest
/// enclosing arrow/function expression and inspects the `CallExpression` it is
/// an argument to.
pub fn new_is_throw_assertion_subject(
    new_node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(new_node_id) {
        match ancestor.kind() {
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                let parent = nodes.parent_node(ancestor.id());
                let AstKind::CallExpression(call) = parent.kind() else {
                    return false;
                };
                return is_throw_assertion_callee(&call.callee)
                    || is_expect_throw_assertion(parent, &call.callee, semantic);
            }
            _ => {}
        }
    }
    false
}

/// True when `callee` names a throw-assertion: a member access (`t.throws`,
/// `assert.rejects`) or bare identifier whose name is `throws` / `throwsAsync` /
/// `rejects`.
fn is_throw_assertion_callee(callee: &Expression) -> bool {
    let name = match callee {
        Expression::StaticMemberExpression(member) => member.property.name.as_str(),
        Expression::Identifier(ident) => ident.name.as_str(),
        _ => return false,
    };
    matches!(name, "throws" | "throwsAsync" | "rejects")
}

/// True when `call` is `expect(cb)` and the `expect(...)` result feeds a throw
/// matcher: either Jest's direct `.toThrow*` member access, or Chai's `.throw` /
/// `.throws` member reached through a chain of intermediate member accesses
/// (`expect(cb).to.throw()`, `expect(cb).to.not.throw()`).
fn is_expect_throw_assertion(
    call_node: &oxc_semantic::AstNode,
    callee: &Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Expression::Identifier(ident) = callee else {
        return false;
    };
    if ident.name.as_str() != "expect" {
        return false;
    }
    // Walk the member-access chain hanging off `expect(cb)`. Jest matches at the
    // first member (`.toThrow`); Chai matches at a `.throw`/`.throws` member that
    // may sit several `.` hops past `expect(cb)` (`.to.throw`).
    let mut current = call_node.id();
    loop {
        let parent = semantic.nodes().parent_node(current);
        let AstKind::StaticMemberExpression(member) = parent.kind() else {
            return false;
        };
        let name = member.property.name.as_str();
        if name.starts_with("toThrow") || matches!(name, "throw" | "throws") {
            return true;
        }
        current = parent.id();
    }
}
