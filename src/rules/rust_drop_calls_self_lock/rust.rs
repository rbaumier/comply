//! rust-drop-calls-self-lock backend.
//!
//! For each `impl Drop for T` block, walk the body for `call_expression`
//! nodes whose function is a `field_expression` of the form
//! `self.<field>.lock()` / `.read()` / `.write()` / `.try_lock()` /
//! `.try_read()` / `.try_write()`. These are the standard Mutex/RwLock
//! acquisition methods; doing so in `Drop` deadlocks if the lock is
//! already held on the dropping thread.
//!
//! A single-level `self.<field>` receiver is exempt when `<field>` is a
//! borrowed reference (`&T` / `&'a T` / `&mut T`) in the Drop target struct
//! defined in the same file: the locked mutex lives outside `self`, so the
//! struct cannot hold it and self-deadlock. Owned mutex fields (`Mutex<T>`,
//! `Arc<Mutex<T>>`), nested receivers (`self.a.b.lock()`), and a struct not
//! resolvable in the file all stay flagged.
//!
//! Test-only `Drop` impls are exempt: a fixture that locks shared state in
//! `Drop` to record drop order for assertions is intentional instrumentation,
//! not a production deadlock. The exemption fires when the impl is gated by
//! `#[cfg(test)]` (on the impl or an enclosing `mod`/`fn`), under a `#![cfg(test)]`
//! file, located in a `tests/` directory, or belonging to a dedicated
//! test-helper crate (`[package].name` ending in `-test`/`-testing`/`-testkit`/
//! `-test-util`/`-test-utils`, e.g. `tower-test`), whose whole source is test
//! infrastructure and is not `#[cfg(test)]`-gated.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{has_test_attribute, is_in_test_context, is_under_tests_dir};

const KINDS: &[&str] = &["impl_item"];

const LOCK_METHODS: &[&str] = &["lock", "read", "write", "try_lock", "try_read", "try_write"];

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
        // Test-only `Drop` impls legitimately lock shared state to record drop
        // order for assertions. Exempt them; a production `Drop` that locks
        // `self` still fires. A dedicated test-helper crate (e.g. `tower-test`)
        // is the test infrastructure itself, so its lock-in-`Drop` teardown is
        // intentional even though the source is not `#[cfg(test)]`-gated.
        if is_in_test_context(node, source_bytes)
            || has_test_attribute(node, source_bytes)
            || is_under_tests_dir(ctx.path)
            || ctx
                .project
                .nearest_cargo_manifest(ctx.path)
                .is_some_and(|m| m.is_test_helper())
        {
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
                    && !locks_borrowed_self_field(node, receiver, source_bytes)
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

/// True when the lock receiver is exactly `self.<field>` and `<field>` is a
/// borrowed reference in the Drop target struct — the locked mutex is external
/// to `self`, so no self-deadlock is possible. `impl_item` is the enclosing
/// `impl Drop for …` node (the `visit_node` argument).
///
/// Only a single-level `self.<field>` receiver is resolved; nested chains
/// (`self.a.b.lock()`) and bare `self.lock()` keep the conservative behavior
/// (flag), since the locked object's ownership can't be resolved structurally.
fn locks_borrowed_self_field(
    impl_item: tree_sitter::Node,
    receiver: tree_sitter::Node,
    source: &[u8],
) -> bool {
    if receiver.kind() != "field_expression" {
        return false;
    }
    let Some(base) = receiver.child_by_field_name("value") else {
        return false;
    };
    if base.kind() != "self" {
        return false;
    }
    let Some(field) = receiver
        .child_by_field_name("field")
        .and_then(|f| f.utf8_text(source).ok())
    else {
        return false;
    };
    crate::rules::rust_helpers::drop_impl_field_is_reference(impl_item, field, source)
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

    /// Run on a file in `dir/src/x.rs` next to the given `Cargo.toml`, so
    /// `nearest_cargo_manifest` resolves the temp crate's manifest (e.g. for
    /// the test-helper-crate exemption).
    fn run_on_with_cargo(cargo_toml_contents: &str, source: &str) -> Vec<Diagnostic> {
        use std::fs;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml_contents).unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        let src_path = dir.path().join("src/x.rs");
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
    }

    #[test]
    fn flags_lock_on_self_field_in_drop() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { let _g = self.m.lock(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_write_on_self_field_in_drop() {
        let source =
            "struct A; impl Drop for A { fn drop(&mut self) { let _g = self.rw.write(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_lock_on_external_in_drop() {
        let source = "struct A; impl Drop for A { fn drop(&mut self) { let _g = OTHER.lock(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_lock_on_self_in_other_impl() {
        let source = "struct A; impl A { fn f(&self) { let _g = self.m.lock(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_try_lock_on_self() {
        let source =
            "struct A; impl Drop for A { fn drop(&mut self) { let _ = self.m.try_lock(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    // Regression for #5545 (gfx-rs/wgpu `UsageScope`): `self.pool` is a borrowed
    // `&'a Mutex<…>`, not an owned mutex, so locking it in `Drop` pushes into an
    // external pool that outlives `self` — no self-deadlock. Must not fire.
    #[test]
    fn allows_lock_on_borrowed_self_field() {
        let source = "struct UsageScope<'a> { pool: &'a Mutex<Vec<u8>> } \
            impl<'a> Drop for UsageScope<'a> { \
                fn drop(&mut self) { self.pool.lock().push(1); } \
            }";
        assert!(run_on(source).is_empty());
    }

    // Load-bearing positive: an OWNED `Mutex<T>` field (`generic_type`, not a
    // `reference_type`) locked in `Drop` is the genuine self-deadlock the rule
    // targets and must STILL fire.
    #[test]
    fn flags_lock_on_owned_mutex_self_field() {
        let source = "struct A { m: Mutex<u8> } \
            impl Drop for A { fn drop(&mut self) { let _g = self.m.lock(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    // An owned `Arc<Mutex<T>>` field is also `generic_type`, not a reference;
    // the struct co-owns the lock, so it must STILL fire.
    #[test]
    fn flags_lock_on_owned_arc_mutex_self_field() {
        let source = "struct A { m: Arc<Mutex<u8>> } \
            impl Drop for A { fn drop(&mut self) { let _g = self.m.lock(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    // Fail-closed: when the struct definition is not in the same file, the field
    // type can't be resolved, so the conservative behavior (flag) is kept.
    #[test]
    fn flags_lock_on_self_field_when_struct_not_in_file() {
        let source =
            "impl Drop for A { fn drop(&mut self) { let _g = self.m.lock(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    // Regression for #1523: a test fixture locking shared state in `Drop` to
    // record drop order for assertions is intentional instrumentation, not a
    // production deadlock — the `#[cfg(test)]` module must not be flagged.
    #[test]
    fn allows_lock_on_self_in_cfg_test_module() {
        let source = "#[cfg(test)] mod tests { \
            struct MayPanicInDrop { drop_log: Arc<Mutex<Vec<u8>>>, id: u8 } \
            impl Drop for MayPanicInDrop { \
                fn drop(&mut self) { let mut g = self.drop_log.lock().unwrap(); g.push(self.id); } \
            } \
        }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_lock_on_self_with_cfg_test_on_impl() {
        let source = "struct A; #[cfg(test)] impl Drop for A { fn drop(&mut self) { let _g = self.m.lock(); } }";
        assert!(run_on(source).is_empty());
    }

    // Negative-space guard: a PRODUCTION `Drop` impl that locks `self` must
    // STILL fire even when it lives in a module that merely declares a nested
    // `#[cfg(test)]` test submodule alongside it.
    #[test]
    fn flags_lock_on_self_in_production_drop_beside_test_module() {
        let source = "struct A; \
            impl Drop for A { fn drop(&mut self) { let _g = self.m.lock(); } } \
            #[cfg(test)] mod tests { fn t() {} }";
        assert_eq!(run_on(source).len(), 1);
    }

    // Regression for #4444: `tower-test`'s mock teardown locks `self.state` in
    // `Drop`. The crate is a dedicated test helper (consumed only as a
    // dev-dependency); its source is not `#[cfg(test)]`-gated, so the only
    // signal is the `[package].name` suffix. Must not fire.
    const TOWER_TEST_DROP: &str =
        "impl<T, U> Drop for Mock<T, U> { fn drop(&mut self) { let _g = self.state.lock(); } }";

    #[test]
    fn allows_lock_on_self_in_test_helper_crate() {
        let cargo = "[package]\nname = \"tower-test\"\nversion = \"0.1.0\"\n";
        assert!(run_on_with_cargo(cargo, TOWER_TEST_DROP).is_empty());
    }

    // Load-bearing negative: the SAME lock-in-`Drop` in a PRODUCTION crate
    // (name `tower`, no test-helper suffix) must STILL fire — the exemption is
    // crate-specific, not a blanket suppression of a serious-deadlock rule.
    #[test]
    fn flags_lock_on_self_in_production_crate() {
        let cargo = "[package]\nname = \"tower\"\nversion = \"0.1.0\"\n";
        assert_eq!(run_on_with_cargo(cargo, TOWER_TEST_DROP).len(), 1);
    }
}
