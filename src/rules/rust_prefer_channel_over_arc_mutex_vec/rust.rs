//! rust-prefer-channel-over-arc-mutex-vec backend.
//!
//! Flags a `let`-bound `Arc::new(Mutex::new(Vec::new()))` construction — the
//! transient fan-in collector shape: a local Vec cloned into spawned tasks that
//! each `.push(` a result, drained once by the parent. Gated on the file also
//! containing `.lock()` and `.push(`, and on the enclosing function containing a
//! `spawn(` call (`thread::spawn`, `tokio::spawn`, …) — the actual concurrency
//! signal. Only the local `let` initializer matches; a `Arc<Mutex<Vec<_>>>` type
//! annotation or a construction used elsewhere (struct-field initializer,
//! closure body, return, argument) declares persistent shared state, not the
//! fan-in local, so it is left alone. Without any spawn the accumulator is
//! single-threaded (e.g. captured in a synchronously-invoked callback) and a
//! channel is not the right replacement, so it is left alone.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.source_contains(".lock()") || !ctx.source_contains(".push(") { return; }

    if !is_arc_mutex_vec_call(node, source) { return; }
    if !is_local_let_initializer(node) { return; }
    if !enclosing_fn_spawns(node, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Use `mpsc::channel` instead of `Arc<Mutex<Vec>>` to collect results from concurrent tasks.".into(),
        Severity::Error,
    ));
}

fn fn_text<'a>(call: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    call.child_by_field_name("function")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("")
}

fn first_arg(call: tree_sitter::Node) -> Option<tree_sitter::Node> {
    call.child_by_field_name("arguments")
        .and_then(|args| args.named_child(0))
}

fn is_arc_path(text: &str) -> bool {
    text == "Arc::new" || text == "std::sync::Arc::new"
}

fn is_mutex_path(text: &str) -> bool {
    text == "Mutex::new" || text == "std::sync::Mutex::new" || text == "parking_lot::Mutex::new"
}

fn is_vec_ctor(call: tree_sitter::Node, source: &[u8]) -> bool {
    let t = fn_text(call, source);
    t == "Vec::new" || t == "std::vec::Vec::new" || t == "Vec::with_capacity"
}

fn is_arc_mutex_vec_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if !is_arc_path(fn_text(node, source)) {
        return false;
    }
    let Some(inner) = first_arg(node) else {
        return false;
    };

    let (mutex_call, target) = if inner.kind() == "call_expression" {
        (inner, first_arg(inner))
    } else {
        return false;
    };
    if !is_mutex_path(fn_text(mutex_call, source)) {
        return false;
    }
    let Some(target) = target else {
        return false;
    };

    match target.kind() {
        "call_expression" => is_vec_ctor(target, source),
        "macro_invocation" => target
            .child_by_field_name("macro")
            .and_then(|m| m.utf8_text(source).ok())
            .is_some_and(|t| t == "vec"),
        _ => false,
    }
}

/// True when `node` is the `value` initializer of a local `let` binding
/// (`let x = <node>;`). A struct-field initializer, closure body, return
/// expression, or call argument is not the fan-in collector local, so they
/// fail this check.
fn is_local_let_initializer(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "let_declaration" {
        return false;
    }
    parent.child_by_field_name("value").map(|v| v.id()) == Some(node.id())
}

/// True when the `function_item` enclosing `node` contains a `spawn(` call
/// (`thread::spawn`, `tokio::spawn`, `task::spawn`, `scope.spawn`, …). A
/// fan-in collector across spawned tasks needs `Arc<Mutex<Vec>>`; without any
/// spawn the accumulator is single-threaded (e.g. captured in a synchronously-
/// invoked callback) and a channel is not the right replacement.
fn enclosing_fn_spawns(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "function_item" {
            let Ok(text) = parent.utf8_text(source) else {
                return false;
            };
            return text_contains_spawn_call(text);
        }
        cur = parent;
    }
    false
}

/// True when `text` contains a `spawn(` call as a whole word (the char before
/// `spawn` is start-of-text or a non-identifier byte, excluding `respawn(` /
/// `despawn(`).
fn text_contains_spawn_call(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(rel) = text[from..].find("spawn(") {
        let pos = from + rel;
        let boundary = pos == 0
            || !matches!(bytes[pos - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_');
        if boundary {
            return true;
        }
        from = pos + "spawn(".len();
    }
    false
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_arc_mutex_vec_with_push() {
        let src = "fn go() { let results = Arc::new(Mutex::new(Vec::new())); let r = results.clone(); thread::spawn(move || r.lock().unwrap().push(compute())); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_channel() {
        let src = "fn go() { let (tx, rx) = mpsc::channel(); thread::spawn(move || tx.send(compute()).unwrap()); let results: Vec<_> = rx.iter().collect(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_arc_mutex_without_push() {
        assert!(run("fn go() { let x = Arc::new(Mutex::new(Vec::new())); }").is_empty());
    }

    #[test]
    fn allows_struct_field_type() {
        let src = "struct IoWorker { wakers: Arc<Mutex<Vec<Waker>>> } fn wake(w: &Mutex<Vec<Waker>>) { w.lock().unwrap().push(noop()); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_type_alias() {
        let src = "type CallbackQueue = Arc<Mutex<Vec<Op>>>; fn drain(q: &Mutex<Vec<Op>>) { q.lock().unwrap().push(op()); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_struct_field_initializer() {
        let src = "fn make() -> S { S { pending: Arc::new(Mutex::new(Vec::new())) } } fn use_it(q: &Mutex<Vec<u8>>) { q.lock().unwrap().push(1); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_once_lock_get_or_init_closure() {
        let src = "static E: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new(); fn get() -> Arc<Mutex<Vec<u8>>> { E.get_or_init(|| Arc::new(Mutex::new(Vec::new()))).clone() } fn use_it(x: &Mutex<Vec<u8>>) { x.lock().unwrap().push(1); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_threaded_callback_accumulator() {
        let src = "fn f() { let acc = Arc::new(Mutex::new(Vec::new())); let acc_c = acc.clone(); let cb = Box::new(move |s| { acc_c.lock().unwrap().push(s); }); call_sync(cb); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_tokio_spawn_fan_in() {
        let src = "fn go() { let results = Arc::new(Mutex::new(Vec::new())); let r = results.clone(); tokio::spawn(async move { r.lock().unwrap().push(compute()); }); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_respawn_only_accumulator() {
        let src = "fn f() { let acc = Arc::new(Mutex::new(Vec::new())); let a = acc.clone(); let cb = move || { a.lock().unwrap().push(1); }; respawn(cb); }";
        assert!(run(src).is_empty());
    }
}
