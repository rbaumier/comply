//! rust-drop-calls-self-lock backend.
//!
//! For each `impl Drop for T` block, walk the body for `call_expression`
//! nodes whose function is a `field_expression` of the form
//! `self.<field>.lock()` / `.read()` / `.write()` / `.try_lock()` /
//! `.try_read()` / `.try_write()`. These are the standard Mutex/RwLock
//! acquisition methods; doing so in `Drop` deadlocks if the lock is
//! already held on the dropping thread.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["impl_item"];

const LOCK_METHODS: &[&str] = &[
    "lock",
    "read",
    "write",
    "try_lock",
    "try_read",
    "try_write",
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
        let bare_trait = trait_text.rsplit("::").next().unwrap_or(trait_text);
        if bare_trait != "Drop" {
            return;
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        let mut cursor = body.walk();
        let mut stack = vec![body];
        while let Some(n) = stack.pop() {
            if n.kind() == "call_expression"
                && let Some(func) = n.child_by_field_name("function")
                && func.kind() == "field_expression"
                && let Some(field) = func.child_by_field_name("field")
            {
                let method = field.utf8_text(source_bytes).unwrap_or("");
                if LOCK_METHODS.contains(&method)
                    && let Some(receiver) = func.child_by_field_name("value")
                    && receiver_starts_with_self(receiver, source_bytes)
                {
                    diagnostics.push(Diagnostic::at_node(
                        std::sync::Arc::clone(&ctx.path_arc),
                        &n,
                        "rust-drop-calls-self-lock",
                        format!(
                            "`Drop::drop` calls `.{method}()` on `self`. \
                             Acquiring a lock in `Drop` can deadlock if the \
                             same lock is held on the dropping thread."
                        ),
                        Severity::Error,
                    ));
                }
            }
            for child in n.children(&mut cursor) {
                stack.push(child);
            }
        }
    }
}

fn receiver_starts_with_self(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else {
        return false;
    };
    text == "self" || text.starts_with("self.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_lock_on_self_field_in_drop() {
        let source =
            "struct A; impl Drop for A { fn drop(&mut self) { let _g = self.m.lock(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_write_on_self_field_in_drop() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { let _g = self.rw.write(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_lock_on_external_in_drop() {
        let source =
            "struct A; impl Drop for A { fn drop(&mut self) { let _g = OTHER.lock(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_lock_on_self_in_other_impl() {
        let source = "struct A; impl A { fn f(&self) { let _g = self.m.lock(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_try_lock_on_self() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { let _ = self.m.try_lock(); } }";
        assert_eq!(run_on(source).len(), 1);
    }
}
