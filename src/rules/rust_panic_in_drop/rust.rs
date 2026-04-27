//! rust-panic-in-drop backend.
//!
//! Walks every `impl Drop for T` block and flags any panic-producing
//! construct inside its `drop` body: `panic!` / `assert!` / `assert_eq!`
//! / `assert_ne!` / `unimplemented!` / `todo!` macro invocations and
//! `.unwrap()` / `.expect(...)` method calls. Panicking from `Drop`
//! during unwinding aborts the process — `Drop` runs on every error
//! path and must be infallible.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["impl_item"];

const PANIC_MACROS: &[&str] = &[
    "panic",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "unimplemented",
    "todo",
    "unreachable",
];

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
                        if PANIC_MACROS.contains(&bare) {
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
                        if name == "unwrap" || name == "expect" {
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_panic_macro_in_drop() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { panic!(\"boom\"); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unwrap_in_drop() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { let _ = self.h.unwrap(); } }";
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
}
