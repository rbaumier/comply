//! rust-panic-in-drop backend.
//!
//! Walks every `impl Drop for T` block and flags any panic-producing
//! construct inside its `drop` body: `panic!` / `assert!` / `assert_eq!`
//! / `assert_ne!` / `unimplemented!` / `todo!` macro invocations and
//! `.unwrap()` / `.expect(...)` method calls. Panicking from `Drop`
//! during unwinding aborts the process — `Drop` runs on every error
//! path and must be infallible.
//!
//! A panic guarded by `if !std::thread::panicking() { ... }` (or the
//! equivalent `if std::thread::panicking() { ... } else { panic!() }`) is
//! exempt: the panic only runs when unwinding is not in progress, so `drop`
//! returns normally and no double-panic abort occurs.
//!
//! A `Drop` impl nested inside a diverging function (`fn … -> !`) is also
//! exempt: that is the no_std double-panic abort idiom (`let _a = Abort;
//! panic!()` unwinds, runs the `Drop`, and the second panic aborts the
//! process), and the rule's "return instead" advice is impossible in a
//! function that can never return.
//!
//! A `Drop` impl whose target type names the drop-bomb idiom
//! (`PanicOnDrop`, `AbortOnDrop`, `DropBomb`, or any `*Bomb`) is exempt:
//! the panic is the type's declared contract — the guard is armed before
//! an uninterruptible operation and defused on success via `mem::forget`,
//! a sibling call this `Drop`-scoped walk cannot see, so the panic fires
//! only when the operation was abandoned, where aborting is intended.
//!
//! A `Drop` impl in a test context — inside a `#[cfg(test)]` module, a
//! `#[test]` function, or a `#![cfg(test)]` file — is exempt: it is compiled
//! out of production binaries, and test fixtures routinely `unwrap`/`panic!`
//! in `Drop` to assert teardown state, where the harness catches the panic and
//! no production unwinding is aborted.
//!
//! A `.unwrap()` / `.expect(...)` whose receiver is proven non-empty by an
//! enclosing `if <recv>.is_some() { … }` / `if <recv>.is_ok() { … }` guard on
//! the same receiver is exempt: the branch body runs only when the value is
//! `Some`/`Ok`, so the unwrap is infallible there and cannot panic.
//!
//! A statement-level panic/unwrap/expect preceded by an early-return guard
//! `if <cond> { … return … }` — whose then-branch diverges via `return` and
//! whose `<cond>` references `std::thread::panicking()` as the whole condition
//! or as a `||` disjunct — is exempt: control reaches the op only when every
//! operand is false, i.e. the thread is not unwinding. The guard may be a direct
//! preceding sibling or sit at the top level of a preceding `unsafe`/plain block
//! (a `return` there still exits `drop`). A `&&` guard does not qualify, since
//! fall-through can still occur while panicking.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["impl_item"];

const PANIC_MACROS: &[&str] = &[
    "panic",
    "assert",
    "assert_eq",
    "assert_ne",
    "unimplemented",
    "todo",
    "unreachable",
];

/// True when `node` sits inside a branch that only runs while the thread is
/// not unwinding, walking ancestors up to `body` (the `drop` body). Two
/// equivalent guards qualify, both reached when `panicking()` is `false`:
/// the consequence of `if !panicking() { … }` and the `else` branch of
/// `if panicking() { … } else { … }`.
fn is_guarded_by_not_panicking(
    node: tree_sitter::Node,
    body: tree_sitter::Node,
    source: &[u8],
) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent == body {
            return false;
        }
        if parent.kind() == "if_expression"
            && let Some(condition) = parent.child_by_field_name("condition")
        {
            if let Some(consequence) = parent.child_by_field_name("consequence")
                && consequence == cur
                && is_negated_panicking_call(condition, source)
            {
                return true;
            }
            if let Some(alternative) = parent.child_by_field_name("alternative")
                && alternative == cur
                && alternative.kind() == "else_clause"
                && is_bare_panicking_call(condition, source)
            {
                return true;
            }
        }
        cur = parent;
    }
    false
}

/// True when `condition` proves the thread is not unwinding: either `!<expr>`
/// where `<expr>` is a bare `panicking()` call (see [`is_bare_panicking_call`]),
/// or a top-level `&&` whose left or right operand satisfies this predicate.
/// A `&&` body runs only when both operands hold, so a negated `panicking()`
/// in either operand still guarantees `drop` is not running while unwinding.
/// `||` is rejected: its body can run while panicking via the other operand.
fn is_negated_panicking_call(condition: tree_sitter::Node, source: &[u8]) -> bool {
    if condition.kind() == "binary_expression" {
        let Some(op) = condition.child_by_field_name("operator") else {
            return false;
        };
        if op.utf8_text(source).unwrap_or("") != "&&" {
            return false;
        }
        let left = condition.child_by_field_name("left");
        let right = condition.child_by_field_name("right");
        return left.is_some_and(|n| is_negated_panicking_call(n, source))
            || right.is_some_and(|n| is_negated_panicking_call(n, source));
    }
    if condition.kind() != "unary_expression" {
        return false;
    }
    let Some(op) = condition.child(0) else {
        return false;
    };
    if op.utf8_text(source).unwrap_or("") != "!" {
        return false;
    }
    let Some(operand) = condition.named_child(0) else {
        return false;
    };
    is_bare_panicking_call(operand, source)
}

/// True when `expr` is a call whose function path ends in the `panicking`
/// segment (`std::thread::panicking()`, `thread::panicking()`, or an
/// imported `panicking()`).
fn is_bare_panicking_call(expr: tree_sitter::Node, source: &[u8]) -> bool {
    if expr.kind() != "call_expression" {
        return false;
    }
    let Some(func) = expr.child_by_field_name("function") else {
        return false;
    };
    let last_segment = match func.kind() {
        "identifier" => func.utf8_text(source).unwrap_or(""),
        "scoped_identifier" => func
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or(""),
        _ => return false,
    };
    last_segment == "panicking"
}

/// True when a statement-level panic/unwrap/expect `node` in the drop body is
/// made unreachable during an unwind by a *preceding* early-return guard. Walks
/// the block ancestors of `node` up to `body` and inspects the siblings that run
/// unconditionally before it: an `if <cond> { … return … }` whose then-branch
/// diverges via `return` and whose `<cond>` references `std::thread::panicking()`
/// as the whole condition or as a `||` disjunct. Falling through such a guard
/// requires every operand false, so the thread is not unwinding when the op runs.
/// The guard may be a direct preceding sibling or sit at the top level of a
/// preceding `unsafe`/plain block sibling (a `return` there still exits `drop`).
///
/// The polarity is the inverse of the nested case in
/// [`is_guarded_by_not_panicking`]: here a *bare* `if panicking() { return }`
/// guards and a `||` disjunct guards, whereas `&&` does not — fall-through can
/// still occur while panicking when the other operand is false.
fn is_guarded_by_preceding_early_return(
    node: tree_sitter::Node,
    body: tree_sitter::Node,
    source: &[u8],
) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "block" {
            let mut sibling = cur.prev_named_sibling();
            while let Some(prev) = sibling {
                if preceding_statement_guards(prev, source) {
                    return true;
                }
                sibling = prev.prev_named_sibling();
            }
        }
        if parent == body {
            return false;
        }
        cur = parent;
    }
    false
}

/// True when `statement` — which executes unconditionally before the fallible
/// op — is, or directly contains, an early-return `panicking()` guard. A
/// preceding `unsafe`/plain block is unwrapped so a guard at its top level is
/// seen, since a `return` inside it still exits `drop`.
fn preceding_statement_guards(statement: tree_sitter::Node, source: &[u8]) -> bool {
    let core = unwrap_expression_statement(statement);
    if core.kind() == "if_expression" {
        return is_early_return_panicking_guard(core, source);
    }
    if let Some(block) = block_body(core) {
        let mut cursor = block.walk();
        return block.named_children(&mut cursor).any(|child| {
            let inner = unwrap_expression_statement(child);
            inner.kind() == "if_expression" && is_early_return_panicking_guard(inner, source)
        });
    }
    false
}

/// True when `if_node` is an `if <cond> { … return … }` early-return guard whose
/// then-branch diverges via a top-level `return` and whose `<cond>` references
/// `std::thread::panicking()` (see [`condition_is_panicking_disjunct`]).
fn is_early_return_panicking_guard(if_node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(condition) = if_node.child_by_field_name("condition") else {
        return false;
    };
    let Some(consequence) = if_node.child_by_field_name("consequence") else {
        return false;
    };
    condition_is_panicking_disjunct(condition, source) && block_diverges_via_return(consequence)
}

/// True when `condition` is a bare `panicking()` call (the whole condition) or a
/// `||` disjunct containing one (`<other> || panicking()`, any position in a
/// `||` chain). Falling through a `{ return }` under such a condition requires
/// every operand false, so the thread is not unwinding. `&&` is rejected: its
/// fall-through occurs whenever any operand is false, which can still be while
/// panicking. A negated `!panicking()` is not a bare call, so it does not match.
fn condition_is_panicking_disjunct(condition: tree_sitter::Node, source: &[u8]) -> bool {
    if is_bare_panicking_call(condition, source) {
        return true;
    }
    if condition.kind() != "binary_expression" {
        return false;
    }
    let Some(op) = condition.child_by_field_name("operator") else {
        return false;
    };
    if op.utf8_text(source).unwrap_or("") != "||" {
        return false;
    }
    let left = condition.child_by_field_name("left");
    let right = condition.child_by_field_name("right");
    left.is_some_and(|n| condition_is_panicking_disjunct(n, source))
        || right.is_some_and(|n| condition_is_panicking_disjunct(n, source))
}

/// True when `block` has a top-level `return`, so reaching it makes the
/// enclosing `drop` diverge. Only direct statements count: a `return` nested in
/// an inner branch does not unconditionally exit.
fn block_diverges_via_return(block: tree_sitter::Node) -> bool {
    let mut cursor = block.walk();
    block
        .named_children(&mut cursor)
        .any(|child| unwrap_expression_statement(child).kind() == "return_expression")
}

/// The inner `block` of an `unsafe { … }` or a bare `{ … }` statement, or `None`
/// for any other node.
fn block_body(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    match node.kind() {
        "block" => Some(node),
        "unsafe_block" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .find(|child| child.kind() == "block")
        }
        _ => None,
    }
}

/// The inner expression of an `expression_statement`, or `node` itself when it is
/// not such a wrapper. Statement-position expressions ending in a block (`if`,
/// `unsafe`, `{ … }`) and `expr;` statements are wrapped in an
/// `expression_statement`; unwrapping exposes the underlying construct.
fn unwrap_expression_statement(node: tree_sitter::Node) -> tree_sitter::Node {
    if node.kind() == "expression_statement" {
        node.named_child(0).unwrap_or(node)
    } else {
        node
    }
}

/// True when `call` is an `<recv>.unwrap()` / `.expect(...)` whose receiver is
/// proven non-empty by an enclosing `if <recv>.is_some() { … }` /
/// `if <recv>.is_ok() { … }` guard on the *same* receiver text, walking
/// ancestors up to `body` (the `drop` body). The call must sit in the guard's
/// `consequence`: the `else` branch is the negated case (`None`/`Err`), where
/// the unwrap would still panic. Matching is on the receiver's source text
/// (`self.itr`), so a guard on a different receiver does not exempt the unwrap.
fn is_guarded_by_some_or_ok(
    call: tree_sitter::Node,
    body: tree_sitter::Node,
    source: &[u8],
) -> bool {
    let Some(receiver) = unwrap_receiver(call) else {
        return false;
    };
    let Ok(receiver_text) = receiver.utf8_text(source) else {
        return false;
    };
    let mut cur = call;
    while let Some(parent) = cur.parent() {
        if parent == body {
            return false;
        }
        if parent.kind() == "if_expression"
            && let Some(consequence) = parent.child_by_field_name("consequence")
            && consequence == cur
            && let Some(condition) = parent.child_by_field_name("condition")
            && condition_is_some_or_ok_on(condition, receiver_text, source)
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// The receiver of an `<recv>.unwrap()` / `.expect(...)` call — the `value`
/// field of the method's `field_expression` callee (`self.itr` in
/// `self.itr.unwrap()`). `None` when `call` is not a method call.
fn unwrap_receiver(call: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "field_expression" {
        return None;
    }
    func.child_by_field_name("value")
}

/// True when `condition` is `<recv>.is_some()` / `<recv>.is_ok()` whose receiver
/// source text equals `receiver`. Bounded to a single method call — a compound
/// condition (`x.is_some() && y`) is not matched, since the body then runs under
/// a broader condition this narrow check does not model.
fn condition_is_some_or_ok_on(
    condition: tree_sitter::Node,
    receiver: &str,
    source: &[u8],
) -> bool {
    if condition.kind() != "call_expression" {
        return false;
    }
    let Some(func) = condition.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "field_expression" {
        return false;
    }
    let method_matches = func
        .child_by_field_name("field")
        .and_then(|f| f.utf8_text(source).ok())
        .is_some_and(|name| name == "is_some" || name == "is_ok");
    let receiver_matches = func
        .child_by_field_name("value")
        .and_then(|v| v.utf8_text(source).ok())
        .is_some_and(|text| text == receiver);
    method_matches && receiver_matches
}

/// True when the first `function_item` enclosing `node` is diverging — its
/// `return_type` is the never type `!`. Only the immediate enclosing function
/// counts: a non-diverging helper nested inside a `-> !` function is not
/// exempt. The `drop` method is a child of the `impl_item`, not an ancestor,
/// so the ancestor walk reaches the enclosing function past it.
fn impl_is_in_diverging_fn(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item" {
            return parent
                .child_by_field_name("return_type")
                .is_some_and(|ret| {
                    ret.kind() == "never_type" || ret.utf8_text(source).map(str::trim) == Ok("!")
                });
        }
        cur = parent;
    }
    false
}

/// Type names whose `Drop` panic is the type's declared contract — a
/// "drop bomb" / panic guard, not an accidental cleanup panic. Such a
/// guard is armed before an operation that must not be interrupted and
/// defused on the happy path (typically `mem::forget`, a sibling call
/// the rule cannot see). The panic only fires when the operation was
/// abandoned mid-way, where aborting is the intended outcome. Matched on
/// the last `::` segment of the impl's target type, case-sensitively, so
/// only types that self-document the intent are exempt.
const PANIC_GUARD_TYPE_MARKERS: &[&str] = &["PanicOnDrop", "AbortOnDrop", "DropBomb"];

/// True when the `impl Drop` target type names itself a panic guard, e.g.
/// `PanicOnDrop`, `AbortOnDrop`, `DropBomb`, or any `*Bomb`. The panic is
/// then the type's purpose, defused on success out of this `Drop`'s scope,
/// so it must not be flagged as an accidental panic-in-drop.
fn is_panic_guard_type(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(type_node) = node.child_by_field_name("type") else {
        return false;
    };
    let type_text = type_node.utf8_text(source).unwrap_or("");
    let name = type_text.rsplit("::").next().unwrap_or(type_text);
    // Strip generic args / lifetimes so `DropBomb<'a>` still matches.
    let name = name.split(['<', ' ']).next().unwrap_or(name);
    PANIC_GUARD_TYPE_MARKERS.contains(&name) || name.ends_with("Bomb")
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(trait_node) = node.child_by_field_name("trait") else {
            return;
        };
        let trait_text = trait_node.utf8_text(source_bytes).unwrap_or("");
        let bare = trait_text.rsplit("::").next().unwrap_or(trait_text);
        if bare != "Drop" {
            return;
        }
        // A `Drop` impl inside a test context — a `#[cfg(test)]` module, a
        // `#[test]` function, or a `#![cfg(test)]` file — is compiled out of
        // production binaries. Test fixtures routinely `unwrap`/`panic!` in
        // `Drop` to assert teardown state; the harness handles those panics and
        // there is no production unwinding to abort, so the rule does not apply.
        if crate::rules::rust_helpers::is_in_test_context(node, source_bytes) {
            return;
        }
        // A `Drop` impl nested inside a diverging function (`fn … -> !`) is the
        // no_std double-panic abort idiom: `let _a = Abort; panic!()` unwinds,
        // runs the `Drop`, and the second panic aborts the process. The "return
        // instead" advice is impossible in a `-> !` function, so do not flag it.
        if impl_is_in_diverging_fn(node, source_bytes) {
            return;
        }
        // A type named after the drop-bomb idiom (`PanicOnDrop`, `AbortOnDrop`,
        // `DropBomb`, `*Bomb`) panics in `Drop` on purpose: it is armed before
        // an uninterruptible operation and defused on success via `mem::forget`
        // — a sibling call this `Drop`-scoped AST walk cannot see. The panic
        // fires only when the operation was abandoned, where abort is intended.
        if is_panic_guard_type(node, source_bytes) {
            return;
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        // Walk body for panic macros and unwrap/expect calls.
        let mut cursor = body.walk();
        let mut stack = vec![body];
        while let Some(n) = stack.pop() {
            match n.kind() {
                "macro_invocation" => {
                    if let Some(m) = n.child_by_field_name("macro") {
                        let name = m.utf8_text(source_bytes).unwrap_or("");
                        let bare = name.rsplit("::").next().unwrap_or(name);
                        if PANIC_MACROS.contains(&bare)
                            && !is_guarded_by_not_panicking(n, body, source_bytes)
                            && !is_guarded_by_preceding_early_return(n, body, source_bytes)
                        {
                            diagnostics.push(Diagnostic::at_node(
                                std::sync::Arc::clone(&ctx.path_arc),
                                &n,
                                "rust-panic-in-drop",
                                format!(
                                    "`{bare}!` inside `Drop::drop`. Panicking \
                                     during unwinding aborts the process — \
                                     log the error and return instead."
                                ),
                                Severity::Error,
                            ));
                        }
                    }
                }
                "call_expression" => {
                    if let Some(func) = n.child_by_field_name("function")
                        && func.kind() == "field_expression"
                        && let Some(field) = func.child_by_field_name("field")
                    {
                        let name = field.utf8_text(source_bytes).unwrap_or("");
                        if (name == "unwrap" || name == "expect")
                            && !is_guarded_by_not_panicking(n, body, source_bytes)
                            && !is_guarded_by_some_or_ok(n, body, source_bytes)
                            && !is_guarded_by_preceding_early_return(n, body, source_bytes)
                        {
                            diagnostics.push(Diagnostic::at_node(
                                std::sync::Arc::clone(&ctx.path_arc),
                                &n,
                                "rust-panic-in-drop",
                                format!(
                                    "`.{name}()` inside `Drop::drop` — panicking \
                                     during unwinding aborts the process. \
                                     Handle the failure explicitly."
                                ),
                                Severity::Error,
                            ));
                        }
                    }
                }
                _ => {}
            }
            for child in n.children(&mut cursor) {
                stack.push(child);
            }
        }
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_panic_macro_in_drop() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { panic!(\"boom\"); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unwrap_in_drop() {
        let source =
            "struct A; impl Drop for A { fn drop(&mut self) { let _ = self.h.unwrap(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_expect_in_drop() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { self.h.expect(\"e\"); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_assert_eq_in_drop() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { assert_eq!(1, 1); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_panic_in_other_impl() {
        let source = "struct A; impl A { fn f(&self) { panic!(\"x\"); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_debug_assert_in_drop() {
        let source = "struct G; impl Drop for G { fn drop(&mut self) { \
                      debug_assert!(self.x.is_empty()); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_debug_assert_eq_in_drop() {
        let source = "struct G; impl Drop for G { fn drop(&mut self) { \
                      debug_assert_eq!(self.x, 0); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_debug_assert_ne_in_drop() {
        let source = "struct G; impl Drop for G { fn drop(&mut self) { \
                      debug_assert_ne!(self.x, 0); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_non_debug_assert_eq_in_drop() {
        let source = "struct G; impl Drop for G { fn drop(&mut self) { \
                      assert_eq!(self.x, 0); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_panic_guarded_by_std_thread_panicking() {
        let source = "struct Child; impl Drop for Child { fn drop(&mut self) { \
                      if !std::thread::panicking() { \
                      panic!(\"Child was dropped before being joined\"); } } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_guarded_by_imported_thread_panicking() {
        let source = "use std::thread; struct Child; impl Drop for Child { \
                      fn drop(&mut self) { if !thread::panicking() { panic!(); } } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_unguarded_panic_in_drop() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { panic!(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_non_negated_panicking_guard() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { \
                      if std::thread::panicking() { panic!(); } } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_compound_and_guard_left_operand() {
        let source = "struct Server; impl Drop for Server { fn drop(&mut self) { \
                      if !std::thread::panicking() && !self.no_hit_checks { \
                      let x = *self.total_hits.lock().unwrap(); \
                      assert!(x > 0, \"test server exited without being called\"); } } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_compound_and_guard_right_operand() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { \
                      if other_cond && !std::thread::panicking() { panic!(\"x\"); } } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_compound_and_guard_nested_chain() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { \
                      if a && !std::thread::panicking() && b { panic!(\"x\"); } } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_compound_or_does_not_guard() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { \
                      if !std::thread::panicking() || other { panic!(\"x\"); } } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_non_negated_panicking_in_compound_and() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { \
                      if std::thread::panicking() && other { panic!(\"x\"); } } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_panic_in_else_of_non_negated_panicking_guard() {
        let source = "struct C; impl Drop for C { fn drop(&mut self) { \
                      if panicking() { eprintln!(\"x\"); } else { panic!(\"y\"); } } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_in_else_of_qualified_panicking_guard() {
        let source = "struct C; impl Drop for C { fn drop(&mut self) { \
                      if std::thread::panicking() { eprintln!(\"x\"); } \
                      else { panic!(\"y\"); } } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_in_drop_inside_diverging_fn_no_std_abort() {
        let source = "fn abort() -> ! { struct Abort; \
                      impl Drop for Abort { fn drop(&mut self) { panic!(); } } \
                      let _a = Abort; panic!(\"abort\"); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_assert_and_unreachable_in_drop_inside_diverging_fn() {
        let source = "fn abort() -> ! { struct Abort; \
                      impl Drop for Abort { fn drop(&mut self) { \
                      assert!(false); unreachable!(); } } \
                      let _a = Abort; panic!(\"abort\"); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_panic_in_drop_inside_non_diverging_fn() {
        let source = "fn foo() { struct X; \
                      impl Drop for X { fn drop(&mut self) { panic!(); } } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_panic_in_panic_on_drop_guard() {
        // slotmap's drop-bomb: defused via `mem::forget` in `clone_from`.
        let source = "pub struct PanicOnDrop(pub &'static str); \
                      impl Drop for PanicOnDrop { fn drop(&mut self) { \
                      panic!(\"{}\", self.0); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_in_abort_on_drop_guard() {
        let source = "struct AbortOnDrop; impl Drop for AbortOnDrop { \
                      fn drop(&mut self) { panic!(\"aborting\"); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_in_drop_bomb_with_generics() {
        let source = "struct DropBomb<'a>(&'a str); impl<'a> Drop for DropBomb<'a> { \
                      fn drop(&mut self) { panic!(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unwrap_in_bomb_suffixed_guard() {
        let source = "struct CommitBomb; impl Drop for CommitBomb { \
                      fn drop(&mut self) { self.h.unwrap(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_panic_in_unrelated_named_drop() {
        // A type that does not declare drop-bomb intent still gets flagged,
        // even with an unconditional hardcoded panic.
        let source = "struct Connection; impl Drop for Connection { \
                      fn drop(&mut self) { panic!(\"cleanup failed\"); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unwrap_in_guard_named_type() {
        // `*Guard` is intentionally NOT exempt: most guards do real cleanup
        // that can accidentally panic.
        let source = "struct MutexGuard; impl Drop for MutexGuard { \
                      fn drop(&mut self) { self.h.unwrap(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_unwrap_guarded_by_is_some_same_receiver() {
        // rust-htslib src/bam/mod.rs:1033 — `self.itr.unwrap()` inside
        // `if self.itr.is_some() { … }` is infallible there.
        let source = "struct R; impl Drop for R { fn drop(&mut self) { unsafe { \
                      if self.itr.is_some() { f(self.itr.unwrap()); } } } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unwrap_guarded_by_is_ok_same_receiver() {
        let source = "struct R; impl Drop for R { fn drop(&mut self) { \
                      if self.h.is_ok() { let _ = self.h.unwrap(); } } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_unguarded_unwrap_in_drop_still() {
        let source = "struct R; impl Drop for R { fn drop(&mut self) { \
                      let _ = self.itr.unwrap(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unwrap_guarded_by_is_some_on_different_receiver() {
        // The guard checks `self.other`, not `self.itr` — `self.itr.unwrap()`
        // can still be `None` and panic.
        let source = "struct R; impl Drop for R { fn drop(&mut self) { \
                      if self.other.is_some() { let _ = self.itr.unwrap(); } } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unwrap_outside_is_some_guard_block() {
        // The `is_some` guard's body ends; the unwrap is a sibling statement
        // after the `if`, so the guard does not reach it.
        let source = "struct R; impl Drop for R { fn drop(&mut self) { \
                      if self.itr.is_some() { g(); } let _ = self.itr.unwrap(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unwrap_in_else_of_is_some_guard() {
        // The `else` branch runs when the value is `None`/`Err` — unwrap panics.
        let source = "struct R; impl Drop for R { fn drop(&mut self) { \
                      if self.itr.is_some() { g(); } else { let _ = self.itr.unwrap(); } } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_unwrap_in_drop_inside_inline_cfg_test_module() {
        // jdx/mise src/github/sigstore.rs: a RAII test fixture that clears test
        // state on drop, nested in an inline `#[cfg(test)] mod tests` in a src
        // file. Compiled out of production, so the unwrap cannot abort a binary.
        let source = "#[cfg(test)] mod tests { struct TokensFileOverrideGuard; \
                      impl Drop for TokensFileOverrideGuard { fn drop(&mut self) { \
                      *TOKENS_FILE_OVERRIDE.write().unwrap() = None; } } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_in_drop_inside_cfg_test_file() {
        // A whole file marked `#![cfg(test)]` is test-only; a Drop fixture
        // panicking there cannot abort a production binary.
        let source = "#![cfg(test)] struct A; \
                      impl Drop for A { fn drop(&mut self) { panic!(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_unwrap_in_production_drop_outside_cfg_test() {
        // The same shape outside any `#[cfg(test)]` module still fires: a
        // production `Drop` panicking during unwinding aborts the process.
        let source = "struct Guard; impl Drop for Guard { fn drop(&mut self) { \
                      *TOKENS_FILE_OVERRIDE.write().unwrap() = None; } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_assert_in_drop_in_cargo_tests_dir() {
        // ndarray tests/iterators.rs: a Drop fixture that asserts a value is
        // dropped exactly once. Panicking-on-drop is the assertion mechanism.
        let source = "struct DropCount; impl Drop for DropCount { \
                      fn drop(&mut self) { assert_eq!(self.my_drops, 0); } }";
        assert!(
            crate::rules::test_helpers::run_rule_gated(&Check, source, "tests/iterators.rs")
                .is_empty()
        );
    }

    #[test]
    fn allows_expect_in_drop_in_cargo_tests_dir() {
        // ndarray tests/array.rs: `.expect("double drop!")` in a Drop fixture.
        let source = "struct InsertOnDrop; impl Drop for InsertOnDrop { \
                      fn drop(&mut self) { self.value.take().expect(\"double drop!\"); } }";
        assert!(
            crate::rules::test_helpers::run_rule_gated(&Check, source, "tests/array.rs").is_empty()
        );
    }

    #[test]
    fn allows_expect_after_preceding_bare_panicking_early_return_in_unsafe() {
        // paradedb pg_search/src/postgres/fake_aminsertcleanup.rs: a bare
        // `if std::thread::panicking() { return; }` precedes the `.expect()`,
        // which is unreachable while the thread is unwinding.
        let source = "struct FrameGuard; impl Drop for FrameGuard { fn drop(&mut self) { \
                      unsafe { if std::thread::panicking() { return; } \
                      let frame = STACK.pop().expect(\"stack underflow\"); } } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_after_or_panicking_early_return_in_preceding_unsafe_sibling() {
        // paradedb pg_search/src/postgres/storage/linked_items.rs: the `panic!()`
        // is a sibling of a preceding `unsafe {}` block whose early-return guard
        // `if !is_tx() || std::thread::panicking() { return; }` bails out while
        // unwinding, so control reaches the panic only when not panicking.
        let source = "struct AtomicGuard; impl Drop for AtomicGuard { fn drop(&mut self) { \
                      if self.original.is_none() { return; } \
                      unsafe { if !pg_sys::IsTransactionState() || std::thread::panicking() \
                      { return; } } panic!(\"failed to call commit()\"); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_panic_after_and_panicking_early_return_guard() {
        // `if cond && panicking() { return; }` does not guard: fall-through
        // happens whenever `cond` is false, which can still be while unwinding.
        let source = "struct A; impl Drop for A { fn drop(&mut self) { \
                      if cond && std::thread::panicking() { return; } panic!(\"x\"); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_panic_with_panicking_early_return_guard_after_the_op() {
        // The guard follows the panic; it cannot make an earlier statement
        // unreachable.
        let source = "struct A; impl Drop for A { fn drop(&mut self) { \
                      panic!(\"x\"); if std::thread::panicking() { return; } } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_panic_after_non_diverging_panicking_guard() {
        // The `panicking()` guard does not `return`, so control still reaches the
        // panic while unwinding.
        let source = "struct A; impl Drop for A { fn drop(&mut self) { \
                      if std::thread::panicking() { eprintln!(\"x\"); } panic!(\"y\"); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_expect_after_negated_panicking_early_return_guard() {
        // `if !panicking() { return; }` returns when *not* unwinding, so the op
        // runs precisely while unwinding — still a double-panic bug.
        let source = "struct A; impl Drop for A { fn drop(&mut self) { \
                      if !std::thread::panicking() { return; } \
                      let _ = self.h.expect(\"e\"); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_panic_after_conditionally_nested_panicking_early_return() {
        // The `panicking()` guard runs only when `cond` holds, so fall-through
        // does not imply not-unwinding — the preceding-sibling check must test
        // the *outer* condition and not recurse into its consequence.
        let source = "struct A; impl Drop for A { fn drop(&mut self) { \
                      if cond { if std::thread::panicking() { return; } } panic!(\"x\"); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_panic_after_deeply_nested_return_in_panicking_guard() {
        // The `return` is nested under an inner `if`, so the guard does not
        // unconditionally diverge — the panic can still run while unwinding.
        let source = "struct A; impl Drop for A { fn drop(&mut self) { \
                      if std::thread::panicking() { if x { return; } } panic!(\"y\"); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_panic_after_panicking_early_return_in_preceding_plain_block() {
        // A bare `{ … }` preceding block is unwrapped like `unsafe { … }`: a
        // top-level early-return guard inside it still exits `drop`.
        let source = "struct A; impl Drop for A { fn drop(&mut self) { \
                      { if std::thread::panicking() { return; } } panic!(\"x\"); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_after_left_operand_or_panicking_early_return() {
        // `panicking()` as the *left* `||` operand guards just as the right does.
        let source = "struct A; impl Drop for A { fn drop(&mut self) { \
                      if std::thread::panicking() || other { return; } panic!(\"x\"); } }";
        assert!(run_on(source).is_empty());
    }
}
