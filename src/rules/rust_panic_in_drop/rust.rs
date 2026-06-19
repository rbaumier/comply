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

/// True when `condition` is `!<expr>` and `<expr>` is a bare `panicking()`
/// call (see [`is_bare_panicking_call`]).
fn is_negated_panicking_call(condition: tree_sitter::Node, source: &[u8]) -> bool {
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
        // A `Drop` impl nested inside a diverging function (`fn … -> !`) is the
        // no_std double-panic abort idiom: `let _a = Abort; panic!()` unwinds,
        // runs the `Drop`, and the second panic aborts the process. The "return
        // instead" advice is impossible in a `-> !` function, so do not flag it.
        if impl_is_in_diverging_fn(node, source_bytes) {
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
}
