//! rust-if-let-mutex-lock backend.
//!
//! Walks `if_let_expression` nodes whose scrutinee is a blocking lock
//! call ending in `.lock()` / `.read()` / `.write()`. Those return a
//! `Result` whose `Err(PoisonError<Guard>)` temporary still wraps a
//! guard, so the guard built by the scrutinee is alive for the entire
//! `if/else` and the `else` branch silently still holds the lock —
//! usually the opposite of what the author intended.
//!
//! The `try_*` family (`try_lock` / `try_read` / `try_write`) is
//! excluded: it returns `Option<Guard>` (or `Result<Guard, _>`) and the
//! `else` branch is the lock-not-acquired path, reached exactly when the
//! scrutinee produced `None`/`Err` — so it never holds a guard.
//!
//! Tree-sitter Rust represents `if let` as an `if_expression` whose
//! `condition` field holds a `let_condition` (or, in some grammar
//! versions, `let_chain`). We accept both shapes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["if_expression"];

const LOCK_METHODS: &[&str] = &["lock", "read", "write"];

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
        // Only flag `if let` (must have an `else` branch + a let condition).
        let Some(condition) = node.child_by_field_name("condition") else {
            return;
        };
        // Find the actual scrutinee inside a let_condition / let_chain.
        let Some(scrutinee) = find_let_scrutinee(condition) else {
            return;
        };
        if !is_lock_call(scrutinee, source_bytes) {
            return;
        }
        // Only flag when there's an `else` branch — otherwise the
        // scope ends with the `if`, which is fine.
        if node.child_by_field_name("alternative").is_none() {
            return;
        }
        if let Some(method) = lock_method_name(scrutinee, source_bytes) {
            diagnostics.push(Diagnostic::at_node(
                std::sync::Arc::clone(&ctx.path_arc),
                &node,
                "rust-if-let-mutex-lock",
                format!(
                    "`if let ... = .{method}()` keeps the guard alive across \
                     the `else` branch. Bind the guard in a separate `let` \
                     before the `if let`, or restructure with `match` + `drop`."
                ),
                Severity::Error,
            ));
        }
    }
}

fn find_let_scrutinee(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    // Walk into let_condition / let_chain looking for a `value`/`scrutinee`
    // field that's a call_expression.
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "let_condition"
            && let Some(v) = n
                .child_by_field_name("value")
                .or_else(|| n.child_by_field_name("scrutinee"))
        {
            return Some(v);
        }
        for c in n.children(&mut cursor) {
            stack.push(c);
        }
    }
    None
}

fn lock_method_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "call_expression" {
        return None;
    }
    // Every blocking `std::sync` lock guard (`Mutex::lock`,
    // `RwLock::read`/`write`) is nullary. A call carrying arguments —
    // e.g. `cache.read(id)` — is a same-named lookup, not a lock guard.
    if node
        .child_by_field_name("arguments")
        .is_some_and(|args| args.named_child_count() > 0)
    {
        return None;
    }
    let func = node.child_by_field_name("function")?;
    if func.kind() != "field_expression" {
        return None;
    }
    let field = func.child_by_field_name("field")?;
    let name = field.utf8_text(source).ok()?;
    if LOCK_METHODS.contains(&name) {
        Some(name)
    } else {
        None
    }
}

fn is_lock_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    lock_method_name(node, source).is_some()
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
    fn flags_if_let_mutex_lock_with_else() {
        let source = "fn f() { if let Ok(g) = m.lock() { go(g); } else { fallback(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_if_let_rwlock_read_with_else() {
        let source = "fn f() { if let Ok(g) = rw.read() { go(g); } else { fallback(); } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_if_let_lock_without_else() {
        let source = "fn f() { if let Ok(g) = m.lock() { go(g); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_match_on_lock() {
        let source = "fn f() { match m.lock() { Ok(g) => go(g), _ => fallback(), } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_if_let_unrelated_call() {
        let source = "fn f() { if let Some(x) = compute() { use_(x); } else { default(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_read_with_argument() {
        // sled: `self.cache.read(node.object_id)` is a cache lookup, not a
        // `std::sync` lock — std lock guards are all nullary.
        let source = "fn f() { if let Some(read_res) = self.cache.read(node.object_id) { use_(read_res); } else { retry(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_write_with_argument() {
        let source = "fn f() { if let Some(x) = self.cache.write(id) { use_(x); } else { retry(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_lock_with_argument() {
        let source = "fn f() { if let Some(x) = registry.lock(key) { use_(x); } else { retry(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_if_let_try_lock_with_else() {
        // sea-orm: the `else` branch is the lock-not-acquired path
        // (`try_lock` returned `None`), so it holds no guard.
        let source = "fn f() { if let Some(mut conn) = self.conn.try_lock() { go(conn); } else { return Err(e); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_if_let_try_read_with_else() {
        let source = "fn f() { if let Ok(g) = rw.try_read() { go(g); } else { fallback(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_if_let_try_write_with_else() {
        let source = "fn f() { if let Ok(g) = rw.try_write() { go(g); } else { fallback(); } }";
        assert!(run_on(source).is_empty());
    }
}
