//! rust-unbounded-channel backend.
//!
//! Matches on the last `::`-separated segment of the called function's
//! path, so predicate methods like `range.is_unbounded()` are not
//! mistaken for channel constructors.
//!
//! Flags:
//! - `unbounded_channel` (tokio's `tokio::sync::mpsc::unbounded_channel`).
//! - `unbounded` (crossbeam's `crossbeam::channel::unbounded`).
//! - `channel` when the file uses `std::sync::mpsc` — `std::sync::mpsc`
//!   has no bounded `channel()` (the bounded one is `sync_channel(N)`),
//!   so a zero-arg `mpsc::channel()` is always unbounded. Tokio's
//!   `mpsc::channel(N)` takes a capacity, so calls with arguments are
//!   left alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{is_in_test_context, is_under_tests_dir};

const KINDS: &[&str] = &["call_expression"];

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
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let Ok(text) = function.utf8_text(source_bytes) else {
            return;
        };
        // Match on the last `::`-separated path segment so a bare predicate
        // method like `range.is_unbounded()` (a `field_expression` whose text
        // happens to end in `unbounded`) is not mistaken for a constructor.
        let last_segment = text.rsplit("::").next().unwrap_or(text);
        let is_unbounded = last_segment == "unbounded_channel"
            || last_segment == "unbounded"
            || last_segment == "channel" && is_inside_mpsc_use(node, source_bytes);
        if !is_unbounded {
            return;
        }
        if is_in_test_context(node, source_bytes) || is_under_tests_dir(ctx.path) {
            return;
        }
        // mpsc::channel — only flag if it's `std::sync::mpsc` (which is
        // always unbounded). Tokio's `mpsc::channel(N)` takes a capacity
        // and is the right call. We distinguish by argument count:
        // unbounded variants take zero args, tokio's bounded variant
        // takes one.
        if last_segment == "channel" {
            let arg_count = node
                .child_by_field_name("arguments")
                .map(|args| {
                    let mut cur = args.walk();
                    args.named_children(&mut cur).count()
                })
                .unwrap_or(0);
            if arg_count > 0 {
                return;
            }
        }
        if is_one_shot_rendezvous(node, source_bytes) {
            return;
        }
        if is_channel_provider_constructor(node, source_bytes) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-unbounded-channel".into(),
            message: format!(
                "`{text}(...)` returns an unbounded queue — a slow \
                 consumer will OOM the process. Use `mpsc::channel(N)` \
                 or `crossbeam::channel::bounded(N)` to get backpressure."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Last-resort heuristic for the bare `channel()` call (no scoping).
/// True if the file has `use std::sync::mpsc` or `use mpsc::*` somewhere.
fn is_inside_mpsc_use(_node: tree_sitter::Node, source: &[u8]) -> bool {
    let text = std::str::from_utf8(source).unwrap_or("");
    text.contains("std::sync::mpsc") || text.contains("use mpsc")
}

/// True when the unbounded construction at `node` is the implementation of a
/// channel *provider* — a `pub fn` that builds the channel and hands the
/// halves back to its caller, so the backpressure decision belongs to the
/// consumer, not here. This is the shape of an unbounded-channel library's own
/// `unbounded`/`channel` constructor (issue #5364: `async_channel::unbounded`).
///
/// Both signals are required so an internal-consumer construction still flags:
/// 1. the enclosing `function_item` carries a `pub` `visibility_modifier`; and
/// 2. its `return_type` exposes a channel half — the return type's subtree
///    names a `Sender`/`Receiver` type (incl. `UnboundedSender`/`Unbounded-
///    Receiver`). A function that constructs an unbounded channel and consumes
///    it internally (stores the receiver in a field, spawns a reader task)
///    returns `Self`/`()`/some unrelated type, so its return type does not name
///    a channel half and it keeps flagging.
fn is_channel_provider_constructor(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(function) = crate::rules::rust_helpers::enclosing_fn(node) else {
        return false;
    };
    if !fn_is_pub(function, source) {
        return false;
    }
    let Some(return_type) = function.child_by_field_name("return_type") else {
        return false;
    };
    return_type_exposes_channel_half(return_type, source)
}

/// True if `function_item` has a direct `visibility_modifier` child whose text
/// begins with `pub` (covers `pub`, `pub(crate)`, `pub(super)`).
fn fn_is_pub(function: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = function.walk();
    function.children(&mut cursor).any(|child| {
        child.kind() == "visibility_modifier"
            && child
                .utf8_text(source)
                .is_ok_and(|t| t.starts_with("pub"))
    })
}

/// True if any `type_identifier` within `return_type` names a channel half —
/// `Sender`, `Receiver`, or their `Unbounded*` variants. Matched by suffix so
/// fully-qualified or aliased names (`mpsc::UnboundedSender<T>`) are covered.
fn return_type_exposes_channel_half(return_type: tree_sitter::Node, source: &[u8]) -> bool {
    let mut found = false;
    walk_type_identifiers(return_type, source, &mut |name| {
        if name.ends_with("Sender") || name.ends_with("Receiver") {
            found = true;
        }
    });
    found
}

/// Invoke `f` on the text of every `type_identifier` node within `subtree`.
fn walk_type_identifiers(
    subtree: tree_sitter::Node,
    source: &[u8],
    f: &mut dyn FnMut(&str),
) {
    let mut cursor = subtree.walk();
    for child in subtree.children(&mut cursor) {
        if child.kind() == "type_identifier"
            && let Ok(text) = child.utf8_text(source)
        {
            f(text);
        }
        walk_type_identifiers(child, source, f);
    }
}

/// True for the one-shot rendezvous shape, where the channel carries at most
/// one in-flight message and so cannot grow without bound:
///
/// ```ignore
/// let (tx, rx) = mpsc::channel();
/// sender.send(tx);   // sender half used exactly once, never cloned/looped
/// rx.recv();         // blocking drain in the same scope
/// ```
///
/// Requires all of:
/// 1. the constructor is the value of a `let (tx, rx) = …` 2-element tuple
///    destructure (otherwise the two halves can't be tracked locally);
/// 2. the sender half is referenced exactly once after binding, and that
///    single use is a genuine consume — not inside a loop, not captured by a
///    nested closure / spawned task, and not re-bound to another name (each
///    of which would let the send run repeatedly or escape tracking and grow
///    the queue without bound) — and the sender is never `.clone()`d;
/// 3. the receiver half is drained — `.recv()`, `.iter()`, `.into_iter()`, or
///    `for _ in rx` — in the same function or closure body.
fn is_one_shot_rendezvous(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(let_decl) = enclosing_let_for_value(node) else {
        return false;
    };
    let Some(pattern) = let_decl.child_by_field_name("pattern") else {
        return false;
    };
    if pattern.kind() != "tuple_pattern" {
        return false;
    }
    let mut cursor = pattern.walk();
    let binders: Vec<tree_sitter::Node> = pattern
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "identifier")
        .collect();
    if binders.len() != 2 {
        return false;
    }
    let (Ok(tx), Ok(rx)) = (binders[0].utf8_text(source), binders[1].utf8_text(source)) else {
        return false;
    };
    let Some(scope) = enclosing_scope_block(let_decl) else {
        return false;
    };
    if sender_clone_count(scope, tx, source) > 0 {
        return false;
    }
    let sender_uses = sender_uses(scope, tx, source);
    if sender_uses.len() != 1 {
        return false;
    }
    let send = sender_uses[0];
    if crate::rules::rust_helpers::is_in_loop_body(send)
        || is_captured_by_closure(send, scope)
        || is_rebound_to_alias(send)
    {
        return false;
    }
    receiver_blocking_recv(scope, rx, source)
}

/// The `let_declaration` whose `value` field is `node`, walking up past the
/// constructor's own wrapper nodes (e.g. `(tx, rx)` destructure). Returns
/// `None` if `node` is not the direct initializer of a `let`.
fn enclosing_let_for_value(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "let_declaration" {
            return (parent.child_by_field_name("value").map(|v| v.id()) == Some(cur.id()))
                .then_some(parent);
        }
        // Stop at statement/expression boundaries: the value of a `let` is a
        // single expression, so we only climb through expression wrappers.
        match parent.kind() {
            "call_expression" | "try_expression" | "await_expression"
            | "reference_expression" | "parenthesized_expression" => cur = parent,
            _ => return None,
        }
    }
    None
}

/// The body `block` of the function or closure that encloses the binding —
/// the block whose parent is a `function_item` or `closure_expression`.
/// Identifier walks start here so they cover the binding's whole lexical
/// scope, including any later statements through which the sender could
/// escape, and stop at the function/closure boundary.
fn enclosing_scope_block(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "block"
            && let Some(grandparent) = parent.parent()
            && matches!(grandparent.kind(), "function_item" | "closure_expression")
        {
            return Some(parent);
        }
        cur = parent;
    }
    None
}

/// Count of identifier nodes in `scope` naming `var` that sit inside a
/// `.clone()` call on it (`tx.clone()`).
fn sender_clone_count(scope: tree_sitter::Node, var: &str, source: &[u8]) -> usize {
    let mut count = 0;
    walk_identifiers(scope, var, source, &mut |id| {
        if let Some(field) = id.parent()
            && field.kind() == "field_expression"
            && let Some(call) = field.parent()
            && call.kind() == "call_expression"
            && let Some(m) = field.child_by_field_name("field")
            && m.utf8_text(source).ok() == Some("clone")
        {
            count += 1;
        }
    });
    count
}

/// The value-use occurrences of `var` in `scope` — every identifier naming
/// `var` that is not the binding occurrence in a `let`/parameter pattern. A
/// move into a call argument (`sender.send(tx)`) and a method call
/// (`tx.send(x)`) each count as one use.
fn sender_uses<'a>(
    scope: tree_sitter::Node<'a>,
    var: &str,
    source: &[u8],
) -> Vec<tree_sitter::Node<'a>> {
    let mut uses = Vec::new();
    walk_identifiers(scope, var, source, &mut |id| {
        if !is_binding_occurrence(id) {
            uses.push(id);
        }
    });
    uses
}

/// True if `use_node` is lexically inside a `closure_expression` that is
/// itself nested within `scope` — i.e. the sender is captured by a closure
/// (or spawned task) that may run the send more than once.
fn is_captured_by_closure(use_node: tree_sitter::Node, scope: tree_sitter::Node) -> bool {
    let mut cur = use_node;
    while let Some(parent) = cur.parent() {
        if parent.id() == scope.id() {
            return false;
        }
        if parent.kind() == "closure_expression" {
            return true;
        }
        cur = parent;
    }
    false
}

/// True if the sole sender use re-binds it to another name (the value of a
/// `let x = tx` or `x = tx` assignment). The sender then escapes under a name
/// this local analysis can't track, so the one-shot reasoning no longer holds.
fn is_rebound_to_alias(use_node: tree_sitter::Node) -> bool {
    use_node.parent().is_some_and(|p| {
        matches!(p.kind(), "let_declaration" | "assignment_expression")
            && p.child_by_field_name("value").map(|v| v.id()) == Some(use_node.id())
    })
}

/// True if `var` is the receiver of a draining call in `scope` — `rx.recv()`,
/// `rx.iter()`, `rx.into_iter()`, or `for _ in rx` — each of which empties the
/// queue.
fn receiver_blocking_recv(scope: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
    let mut drained = false;
    walk_identifiers(scope, var, source, &mut |id| {
        if is_drain_method_call(id, source) || is_for_loop_iterable(id) {
            drained = true;
        }
    });
    drained
}

/// True if `id` is the `value` of a `field_expression` whose method is a
/// queue-draining call (`recv` / `iter` / `into_iter`).
fn is_drain_method_call(id: tree_sitter::Node, source: &[u8]) -> bool {
    id.parent().is_some_and(|field| {
        field.kind() == "field_expression"
            && field.child_by_field_name("value").map(|v| v.id()) == Some(id.id())
            && field
                .parent()
                .is_some_and(|call| call.kind() == "call_expression")
            && field
                .child_by_field_name("field")
                .and_then(|m| m.utf8_text(source).ok())
                .is_some_and(|m| matches!(m, "recv" | "iter" | "into_iter"))
    })
}

/// True if `id` is the iterable of a `for … in id { … }` loop, which drains
/// the receiver via its `IntoIterator` impl.
fn is_for_loop_iterable(id: tree_sitter::Node) -> bool {
    id.parent().is_some_and(|p| {
        p.kind() == "for_expression"
            && p.child_by_field_name("value").map(|v| v.id()) == Some(id.id())
    })
}

/// True if `id` is the name being introduced by a `let`/parameter binding
/// pattern rather than a value-use of it.
fn is_binding_occurrence(id: tree_sitter::Node) -> bool {
    let mut cur = id;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "tuple_pattern" | "mut_pattern" | "ref_pattern" | "reference_pattern" => cur = parent,
            "parameter" => {
                return parent.child_by_field_name("pattern").map(|p| p.id()) == Some(cur.id());
            }
            "let_declaration" | "closure_parameters" | "parameters" => {
                return parent.child_by_field_name("pattern").map(|p| p.id()) == Some(cur.id())
                    || parent_pattern_contains(parent, cur);
            }
            _ => return false,
        }
    }
    false
}

/// Whether `child` is a direct pattern child of `parent` for node kinds
/// (`closure_parameters`/`parameters`) that hold patterns as direct children
/// rather than in a `pattern` field.
fn parent_pattern_contains(parent: tree_sitter::Node, child: tree_sitter::Node) -> bool {
    let mut cursor = parent.walk();
    parent.named_children(&mut cursor).any(|c| c.id() == child.id())
}

/// Invoke `f` on every `identifier` node within `subtree` whose text equals
/// `var`. Descends into all children.
fn walk_identifiers<'a>(
    subtree: tree_sitter::Node<'a>,
    var: &str,
    source: &[u8],
    f: &mut dyn FnMut(tree_sitter::Node<'a>),
) {
    let mut cursor = subtree.walk();
    for child in subtree.children(&mut cursor) {
        if child.kind() == "identifier" && child.utf8_text(source).ok() == Some(var) {
            f(child);
        }
        walk_identifiers(child, var, source, f);
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
    fn flags_tokio_unbounded_channel() {
        let source = "fn f() { let (tx, rx) = tokio::sync::mpsc::unbounded_channel(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_std_mpsc_channel() {
        let source = "use std::sync::mpsc;\nfn f() { let (tx, rx) = mpsc::channel(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_crossbeam_unbounded() {
        let source = "fn f() { let (tx, rx) = crossbeam::channel::unbounded(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_tokio_bounded_channel() {
        let source = "fn f() { let (tx, rx) = tokio::sync::mpsc::channel(1024); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_crossbeam_bounded() {
        let source = "fn f() { let (tx, rx) = crossbeam::channel::bounded(1024); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_std_sync_channel_with_capacity() {
        let source = "use std::sync::mpsc;\nfn f() { let (tx, rx) = mpsc::sync_channel(1024); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unbounded_channel_in_test_fn() {
        let source = "#[test]\nfn it_works() { let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<u8>(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unbounded_channel_in_tokio_test() {
        let source = "#[tokio::test]\nasync fn it_works() { let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<u8>(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unbounded_channel_in_tests_dir() {
        let source = "fn f() { let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<u8>(); }";
        assert!(crate::rules::test_helpers::run_rule(&Check, source, "tests/my_test.rs").is_empty());
    }

    #[test]
    fn allows_is_unbounded_predicate_method() {
        // Issue #3219: `range.is_unbounded()` is a predicate, not a constructor.
        let source =
            "fn f(num_vals: Option<ValueRange>) -> bool { num_vals.unwrap_or_default().is_unbounded() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_check_unbounded_predicate_method() {
        let source = "fn f(x: T) -> bool { x.check_unbounded() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unbounded_field_access_method() {
        let source = "fn f(range: Range) -> bool { range.is_unbounded() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_one_shot_rendezvous() {
        // Issue #4763: sender moved out exactly once, blocking `rx.recv()`
        // drains the single reply — the queue holds at most one message.
        let source = "use std::sync::mpsc;\n\
            fn drop_it(s: &Sender) {\n\
                let (tx, rx) = mpsc::channel();\n\
                if s.send(tx).is_ok() {\n\
                    if let Err(e) = rx.recv() { eprintln!(\"{:?}\", e); }\n\
                }\n\
            }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_when_sender_sends_multiple_times() {
        let source = "use std::sync::mpsc;\n\
            fn f() {\n\
                let (tx, rx) = mpsc::channel();\n\
                tx.send(1).unwrap();\n\
                tx.send(2).unwrap();\n\
                rx.recv().unwrap();\n\
            }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_when_sender_sends_in_loop() {
        let source = "use std::sync::mpsc;\n\
            fn f(items: Vec<u8>) {\n\
                let (tx, rx) = mpsc::channel();\n\
                for i in items { tx.send(i).unwrap(); }\n\
                rx.recv().unwrap();\n\
            }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_when_sender_is_cloned() {
        let source = "use std::sync::mpsc;\n\
            fn f() {\n\
                let (tx, rx) = mpsc::channel();\n\
                let tx2 = tx.clone();\n\
                rx.recv().unwrap();\n\
            }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_when_no_receiver_drain() {
        // Sender used once but the receiver is never drained in this scope —
        // not a rendezvous; keep flagging.
        let source = "use std::sync::mpsc;\n\
            fn f(s: &Sender) {\n\
                let (tx, rx) = mpsc::channel();\n\
                s.send(tx).unwrap();\n\
            }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_when_not_destructured() {
        // No `let (tx, rx)` destructure — can't track halves locally, keep flagging.
        let source = "fn f() { let pair = tokio::sync::mpsc::unbounded_channel(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_sender_moved_into_spawned_thread_looping() {
        // The sender is moved into a spawned thread that sends in a loop — the
        // queue can grow without bound; `is_in_loop_body` sees the loop before
        // the closure boundary, so the loop use is detected. Keep flagging.
        let source = "use std::sync::mpsc;\n\
            fn f() {\n\
                let (tx, rx) = mpsc::channel();\n\
                std::thread::spawn(move || {\n\
                    loop { tx.send(work()).unwrap(); }\n\
                });\n\
                rx.recv().unwrap();\n\
            }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_when_receiver_name_shadowed_without_drain() {
        // Sender used once, but `rx` here is never drained — a same-named local
        // rebinding does not count as a receiver drain. Keep flagging.
        let source = "use std::sync::mpsc;\n\
            fn f(s: &Sender) {\n\
                let (tx, rx) = mpsc::channel();\n\
                let rx = compute(rx);\n\
                s.send(tx).unwrap();\n\
            }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_sender_captured_by_event_callback() {
        // The sender is captured by a callback closure with no syntactic loop:
        // the framework may invoke the callback any number of times, so the
        // queue can grow without bound. A single textual `.send()` inside a
        // captured closure must NOT be treated as one-shot. Keep flagging.
        let source = "use std::sync::mpsc;\n\
            fn f(registry: &Registry) {\n\
                let (tx, rx) = mpsc::channel();\n\
                registry.on_event(move |evt| { tx.send(evt).unwrap(); });\n\
                rx.recv().unwrap();\n\
            }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_when_sender_reassigned_to_alias() {
        // The single sender use re-binds it to another name, through which the
        // real (unbounded) sends happen. comply cannot follow the alias, so it
        // must not treat this as one-shot. Keep flagging.
        let source = "use std::sync::mpsc;\n\
            fn f() {\n\
                let (tx, rx) = mpsc::channel();\n\
                let sender = tx;\n\
                rx.recv().unwrap();\n\
            }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_unbounded_inside_pub_channel_provider_constructor() {
        // Issue #5364: async_channel's own `unbounded()` constructor builds the
        // channel and returns the (Sender, Receiver) halves to its caller — the
        // backpressure decision belongs to the consumer, not the provider.
        let source = "pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {\n\
            let channel = Arc::new(Channel { queue: ConcurrentQueue::unbounded() });\n\
            let s = Sender { channel: channel.clone() };\n\
            let r = Receiver { channel };\n\
            (s, r)\n\
        }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unbounded_in_pub_constructor_returning_sender_only() {
        let source = "pub fn unbounded_channel<T>() -> UnboundedSender<T> {\n\
            let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();\n\
            tx\n\
        }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_unbounded_in_pub_fn_consuming_channel_internally() {
        // The function builds an unbounded channel and consumes the receiver
        // internally (stores it in `Self`); its return type is `Self`, not a
        // channel half, so the consumer never gets to choose — keep flagging.
        let source = "pub fn new() -> Self {\n\
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();\n\
            Self { sender: tx, receiver: rx }\n\
        }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unbounded_in_private_constructor_returning_halves() {
        // Same provider return shape but the function is not `pub`: an internal
        // helper is a consumer-side decision the crate owns — keep flagging.
        let source = "fn make() -> (Sender<T>, Receiver<T>) {\n\
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();\n\
            (tx, rx)\n\
        }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unbounded_in_pub_fn_returning_unit() {
        let source = "pub fn run() {\n\
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();\n\
            tokio::spawn(async move { while let Some(m) = rx.recv().await { handle(m); } });\n\
        }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_rendezvous_drained_by_for_loop() {
        // `for _ in rx` drains the queue via IntoIterator; sender sent once.
        let source = "use std::sync::mpsc;\n\
            fn f(s: &Sender) {\n\
                let (tx, rx) = mpsc::channel();\n\
                s.send(tx).unwrap();\n\
                for _msg in rx { break; }\n\
            }";
        assert!(run_on(source).is_empty());
    }
}
