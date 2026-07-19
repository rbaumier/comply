//! rust-loop-collect-into-existing-vec backend.
//!
//! Match `for_expression` whose body is a single-statement block calling
//! `<receiver>.push(...)` where `<receiver>` is a local binding confirmed to
//! be a `Vec`. We do not require the push argument to be the loop variable —
//! when the push value is infallible and does not read the receiver,
//! `for x in src { v.push(transform(x)); }` is still better written as
//! `v.extend(src.into_iter().map(transform))`.
//!
//! A wildcard loop pattern (`for _ in 0..n`) has no binding to map over, so the
//! push value cannot be a transformation of the loop variable; such loops are a
//! repeat-N-times side effect (`v.push(self.pop())`), not a map, and not flagged.
//!
//! A push value that reads the receiver (`v.push(... v[i] ...)`) is a scan over
//! a self-referential accumulator, not a `map`. A closure passed to `extend`
//! cannot borrow the receiver while `extend` holds `&mut` it, so that rewrite
//! would not compile; such loops are not flagged.
//!
//! A push argument containing a control-flow exit — `?` (a `try_expression`),
//! `continue`, `break`, or `return` — is skipped: each exit acts on the
//! enclosing `for` loop or function and cannot live inside a `map` closure, so
//! `extend(...map(...))` would not compile. A `macro_invocation` is treated the
//! same way: macros are opaque at static-analysis time (we do not expand them),
//! and a try-flavored macro can expand to an early `return`/`?`, so any macro in
//! the push argument is conservatively assumed to affect control flow.
//!
//! A push argument containing an `.await` (an `await_expression`) is skipped:
//! `.await` is only permitted in an `async` context and cannot appear inside
//! the synchronous `Fn` closure passed to `extend`, so `extend(...map(...))`
//! would not compile.
//!
//! A push argument whose value flows through a block, `if`, or `match` arm that
//! runs a `;`-terminated statement (an `expression_statement`, e.g. a nested
//! `other.push(...);`) drives external side effects per element — a
//! parallel-collection / struct-of-arrays build. Lifting those statements into a
//! `map` closure makes the transform impure, so the loop is not a pure
//! `extend(...map(...))` and is not flagged. A `let` declaration and the block's
//! tail expression stay pure, so `dst.push({ let y = f(x); y })` still flags.
//!
//! `.push` exists on many non-`Vec` types (`VecDeque`, crossbeam `Worker`,
//! `Injector`, custom queues), none of which `extend`s the same way, so we
//! only flag when the receiver is an in-scope `let` whose initializer is
//! `Vec`-shaped (`Vec::new()`, `Vec::with_capacity(...)`, `vec![...]`) or
//! that carries an explicit `: Vec<...>` annotation. When the receiver's
//! `Vec`-ness cannot be confirmed locally, we stay silent.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["for_expression"];

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
        let source = ctx.source.as_bytes();
        // A wildcard loop pattern (`for _ in 0..n`) has no binding to map over:
        // the push value cannot be a transformation of the loop variable, so the
        // loop is a repeat-N-times side effect (e.g. `v.push(self.pop())`), not a
        // map over a source collection. The `extend(...map(...))` rewrite does not
        // apply, so leave it untouched.
        if let Some(pattern) = node.child_by_field_name("pattern")
            && pattern.utf8_text(source) == Ok("_")
        {
            return;
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        if body.kind() != "block" {
            return;
        }
        // Single-statement body only — multi-statement loops can have
        // side effects we can't summarize as `extend`.
        let mut cursor = body.walk();
        let stmts: Vec<_> = body.named_children(&mut cursor).collect();
        if stmts.len() != 1 {
            return;
        }
        let stmt = stmts[0];
        let call = match stmt.kind() {
            "expression_statement" => {
                let mut c = stmt.walk();
                stmt.named_children(&mut c).next()
            }
            "call_expression" => Some(stmt),
            _ => None,
        };
        let Some(call) = call else { return };
        if call.kind() != "call_expression" {
            return;
        }
        let Some(function) = call.child_by_field_name("function") else {
            return;
        };
        if function.kind() != "field_expression" {
            return;
        }
        let Some(field) = function.child_by_field_name("field") else {
            return;
        };
        let Ok(method) = field.utf8_text(source) else {
            return;
        };
        if method != "push" {
            return;
        }
        // A push argument containing a control-flow exit (`?`, `continue`,
        // `break`, `return`, or an opaque macro) acts on the enclosing loop or
        // function and can't live inside a `map` closure, so the
        // `extend(...map(...))` rewrite would not compile. Skip it.
        if let Some(arguments) = call.child_by_field_name("arguments")
            && arg_contains_early_exit(arguments)
        {
            return;
        }
        // A push argument containing an `.await` (an `await_expression`) is
        // awaited per element; `.await` is only permitted in an `async` context
        // and cannot appear inside the synchronous `Fn` closure passed to
        // `extend`, so the `extend(...map(...))` rewrite would not compile. Skip
        // it. The walk is scoped to the argument list, so awaiting the source
        // iterable (`for x in fetch().await { .. }`) — where the `map` rewrite
        // still compiles — keeps flagging.
        if let Some(arguments) = call.child_by_field_name("arguments")
            && arg_contains_await(arguments)
        {
            return;
        }
        // A push argument whose value is produced by a block / `if` / `match` arm
        // that runs a `;`-terminated statement (an `expression_statement`, e.g. a
        // nested `other.push(...);`) drives external side effects per element — a
        // parallel-collection / struct-of-arrays build. Lifting those statements
        // into a `map` closure makes the transform impure, so the loop is not a
        // pure `extend(...map(...))`. Skip it. `let` declarations and the tail
        // expression carry no discarded-value statement, so they still flag.
        if let Some(arguments) = call.child_by_field_name("arguments")
            && arg_contains_side_effect_statement(arguments)
        {
            return;
        }
        // Only flag when the receiver is a plain local identifier we can
        // resolve to a confirmable `Vec`. A field access (`self.q.push`) or
        // method chain receiver can't be checked, so it is left untouched.
        let Some(receiver) = function.child_by_field_name("value") else {
            return;
        };
        if receiver.kind() != "identifier" {
            return;
        }
        let Ok(var) = receiver.utf8_text(source) else {
            return;
        };
        if !crate::rules::rust_helpers::local_let_binds_vec(node, var, source) {
            return;
        }
        // A push value that reads the destination vector (`curr.push(curr[j] + 1)`) is
        // a scan/fold over a self-referential accumulator. An `extend(map(closure))`
        // rewrite can't borrow the receiver inside the closure while `extend` holds
        // `&mut` it, so it would not compile — skip it.
        if let Some(arguments) = call.child_by_field_name("arguments")
            && arg_references_receiver(arguments, var, source)
        {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "rust-loop-collect-into-existing-vec",
            "`for x in src { dst.push(...); }` is `dst.extend(src.into_iter().map(...))`. \
             `extend` reserves capacity from `size_hint`; the loop reallocates per element."
                .into(),
            Severity::Error,
        ));
    }
}

/// Whether the subtree rooted at `node` contains a control-flow exit — `?`
/// (`try_expression`), `continue`, `break`, `return`, or a `macro_invocation`.
/// Used on the `push` argument list: such an exit acts on the enclosing loop or
/// function and cannot be expressed inside the `map` closure of
/// `extend(...map(...))`. A `macro_invocation` is opaque — we cannot expand it,
/// and a try-flavored macro can hide an early `return`/`?` — so any macro is
/// conservatively treated as control-flow-affecting (no macro-name allowlist).
fn arg_contains_early_exit(node: tree_sitter::Node) -> bool {
    if matches!(
        node.kind(),
        "try_expression"
            | "continue_expression"
            | "break_expression"
            | "return_expression"
            | "macro_invocation"
    ) {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(arg_contains_early_exit)
}

/// Whether the subtree rooted at `node` contains an `await_expression`. Used on
/// the `push` argument: `.await` is only permitted in an `async` context and
/// cannot appear inside the synchronous `Fn` closure passed to `extend`, so an
/// awaited push value cannot be lifted into `extend(...map(...))`.
fn arg_contains_await(node: tree_sitter::Node) -> bool {
    if node.kind() == "await_expression" {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(arg_contains_await)
}

/// Whether the subtree rooted at `node` contains an `expression_statement` — a
/// `;`-terminated discarded-value statement (e.g. a nested `other.push(...);`)
/// run only for its side effect. Used on the `push` argument: when the pushed
/// value is produced by a block / `if` / `match` arm that runs such statements
/// before its tail expression, the loop drives external side effects per element
/// (a parallel-collection / struct-of-arrays build), so it is not a pure
/// `extend(...map(...))`. A `let_declaration` and the block's tail expression are
/// not `expression_statement`s, so they do not trip this guard.
fn arg_contains_side_effect_statement(node: tree_sitter::Node) -> bool {
    if node.kind() == "expression_statement" {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(arg_contains_side_effect_statement)
}

/// True if any `identifier` in the push-argument subtree equals the receiver
/// name `var` — i.e. the push value reads the destination vector.
fn arg_references_receiver(node: tree_sitter::Node, var: &str, source: &[u8]) -> bool {
    if node.kind() == "identifier" && node.utf8_text(source) == Ok(var) {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if arg_references_receiver(child, var, source) {
            return true;
        }
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
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_for_with_single_push() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { dst.push(x); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_for_with_push_of_transform() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { dst.push(x + 1); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_vec_macro_initializer() {
        let src = "fn f(src: Vec<u32>) { let mut dst = vec![]; for x in src { dst.push(x); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_with_capacity_initializer() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::with_capacity(src.len()); for x in src { dst.push(x); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_explicit_vec_type_annotation() {
        let src = "fn f(src: Vec<u32>) { let mut dst: Vec<u32> = make(); for x in src { dst.push(x); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_for_with_multiple_statements() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { let y = x + 1; dst.push(y); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_for_with_non_push_call() {
        let src = "fn f(src: Vec<u32>) { for x in src { println!(\"{}\", x); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_extend_call() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); dst.extend(src); }";
        assert!(run_on(src).is_empty());
    }

    // The receiver's `Vec`-ness can't be confirmed from a parameter binding,
    // so a push to an unknown receiver type is left alone.
    #[test]
    fn allows_push_to_unconfirmed_param() {
        let src = "fn f(src: Vec<u32>, mut dst: Vec<u32>) { for x in src { dst.push(x); } }";
        assert!(run_on(src).is_empty());
    }

    // crossbeam-deque Worker — push to a concurrent deque, not a Vec.
    #[test]
    fn allows_push_to_worker_deque_issue_1478() {
        let src = "fn f() { let w = Worker::new_fifo(); for i in 1..=3 { w.push(i); } }";
        assert!(run_on(src).is_empty());
    }

    // crossbeam-deque Injector — push to a concurrent queue, not a Vec.
    #[test]
    fn allows_push_to_injector_issue_1478() {
        let src = "fn f() { let q = Injector::new(); for i in 0..200 { q.push(i); } }";
        assert!(run_on(src).is_empty());
    }

    // crossbeam-epoch Queue with explicit type annotation — not a Vec.
    #[test]
    fn allows_push_to_typed_queue_issue_1478() {
        let src = "fn f() { let q: Queue<i64> = Queue::new(); for i in 0..200 { q.push(i) } }";
        assert!(run_on(src).is_empty());
    }

    // A push argument using `?` (fallible) can't be lifted into a `map`
    // closure, so `extend(...map(...))` would not compile.
    #[test]
    fn allows_fallible_push_argument_issue_3803() {
        let src = "fn build(src: &[u32]) -> Result<Vec<u32>, ()> { let mut dst = Vec::with_capacity(src.len()); for x in src { dst.push(transform(*x)?); } Ok(dst) }";
        assert!(run_on(src).is_empty());
    }

    // `?` nested deeper in the push argument is still fallible.
    #[test]
    fn allows_nested_fallible_push_argument_issue_3803() {
        let src = "fn build(src: &[u32]) -> Result<Vec<u32>, ()> { let mut dst = Vec::new(); for x in src { dst.push(transform(parse(x)?)); } Ok(dst) }";
        assert!(run_on(src).is_empty());
    }

    // The genuine target stays flagged: an infallible transform with no `?`.
    #[test]
    fn flags_infallible_transform_push_issue_3803() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { dst.push(transform(x)); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // A match arm uses `continue` to skip the push for one variant; `continue`
    // acts on the `for` loop and can't live in a `map` closure, so
    // `extend(...map(...))` would not compile.
    #[test]
    fn allows_continue_in_push_argument_issue_6387() {
        let src = "fn f(p: P) { let mut segments = vec![]; for sp in p.into_inner() { segments.push(match sp.as_rule() { Rule::identity => continue, Rule::wildcard => Segment::Wildcard, _ => unreachable!() }); } }";
        assert!(run_on(src).is_empty());
    }

    // `break` in the push argument exits the loop — not expressible in a closure.
    #[test]
    fn allows_break_in_push_argument_issue_6387() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { dst.push(if x == 0 { break } else { x }); } }";
        assert!(run_on(src).is_empty());
    }

    // `return` in the push argument exits the function — not expressible in a closure.
    #[test]
    fn allows_return_in_push_argument_issue_6387() {
        let src = "fn f(src: Vec<u32>) -> Vec<u32> { let mut dst = Vec::new(); for x in src { dst.push(if x == 0 { return Vec::new() } else { x }); } dst }";
        assert!(run_on(src).is_empty());
    }

    // Negative control: a `match` push argument with no control-flow exit — the
    // `extend(...map(...))` rewrite is valid, so this still flags.
    #[test]
    fn flags_match_push_without_early_exit_issue_6387() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { dst.push(match x { 0 => 1, n => n }); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // A try-flavored macro (`try_likely_ok!`) expands to an early `return`, so
    // the push argument is fallible; the macro is opaque to comply, so any
    // `macro_invocation` in the argument is skipped.
    #[test]
    fn allows_macro_invocation_in_push_argument_issue_6245() {
        let src = "fn build(s: &str) -> Result<Vec<u32>, ()> { let mut items = Vec::with_capacity(2); for item in Tokenizer::new(s.as_bytes()) { items.push(try_likely_ok!(item)); } Ok(items) }";
        assert!(run_on(src).is_empty());
    }

    // A `macro_invocation` nested deeper in the push argument is still opaque.
    #[test]
    fn allows_nested_macro_invocation_in_push_argument_issue_6245() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { dst.push(wrap(my_macro!(x))); } }";
        assert!(run_on(src).is_empty());
    }

    // Negative control: a plain function call (no macro, no early exit) is a
    // valid `map`, so this still flags.
    #[test]
    fn flags_plain_call_push_argument_issue_6245() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { dst.push(plain_fn(x)); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // Negative control: a method call (no macro, no early exit) is a valid
    // `map`, so this still flags.
    #[test]
    fn flags_method_call_push_argument_issue_6245() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { dst.push(x.method()); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // A push value that reads the receiver is a scan over a self-referential
    // accumulator; `extend(...map(...))` can't borrow the receiver in the
    // closure, so the loop is not flagged.
    #[test]
    fn allows_self_referential_scan_issue_3739() {
        let src = "fn scan(src: &[usize]) -> Vec<usize> { let mut acc: Vec<usize> = Vec::new(); for (i, &x) in src.iter().enumerate() { acc.push(x + acc[i.saturating_sub(1)]); } acc }";
        assert!(run_on(src).is_empty());
    }

    // Levenshtein two-row DP: the push value reads the receiver via an index
    // expression (`curr[j]`), the same self-dependency `extend` can't express.
    #[test]
    fn allows_self_referential_index_read_issue_3739() {
        let src = "fn f(prev: &[usize], b: &[u8], n: usize) -> Vec<usize> { let mut curr: Vec<usize> = Vec::with_capacity(n + 1); for (j, &cb) in b.iter().enumerate() { curr.push((prev[j]).min(curr[j] + 1)); } curr }";
        assert!(run_on(src).is_empty());
    }

    // A wildcard loop pattern (`for _ in 0..n`) has no binding to map over: this
    // is a pop-N-from-stack idiom, not a map over a source collection.
    #[test]
    fn allows_wildcard_loop_pattern_issue_6671() {
        let src = "fn f(&mut self, num_args: u32) -> Vec<u32> { let mut content = Vec::with_capacity(num_args as usize); for _ in 0..num_args { content.push(self.pop()); } content }";
        assert!(run_on(src).is_empty());
    }

    // An underscore-prefixed binding (`_x`) is still a usable binding, distinct
    // from the bare wildcard `_`, so a map-over-source loop stays flagged.
    #[test]
    fn flags_underscore_prefixed_binding_issue_6671() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for _x in src { dst.push(g(_x)); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // The push value is an `if let` whose arms push to a *second* collection
    // (`validity`) before producing the tail value — a parallel-array / SoA build.
    // Those `;`-terminated statements can't be lifted into a pure `map` closure,
    // so the loop is not `extend(...map(...))`.
    #[test]
    fn allows_side_effecting_if_let_arms_issue_7225() {
        let src = "fn f(strings: Vec<Option<u32>>) { let mut cat_ids = Vec::with_capacity(strings.len()); let mut validity = Vec::new(); for opt_s in strings { cat_ids.push(if let Some(cat) = f(opt_s) { validity.push(true); g(cat) } else { validity.push(false); h() }); } }";
        assert!(run_on(src).is_empty());
    }

    // A push value produced by a block that runs a side-effecting statement
    // before its tail expression is not a pure `map`.
    #[test]
    fn allows_side_effecting_block_push_issue_7225() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { dst.push({ side_effect(x); compute(x) }); } }";
        assert!(run_on(src).is_empty());
    }

    // Negative control: a block with only a `let` binding and a tail expression
    // has no discarded-value statement, so it stays a pure `map` and still flags.
    #[test]
    fn flags_let_then_tail_block_push_issue_7225() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { dst.push({ let y = f(x); y }); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // The push argument `p.read().await.clone()` contains an `.await`; `.await`
    // can't live inside a synchronous `map` closure, so `extend(...map(...))`
    // would not compile. The awaited-accumulation loop is left untouched.
    #[test]
    fn allows_awaited_push_argument_issue_7843() {
        let src = "async fn f(xs: &[T]) -> Vec<U> { let mut v = Vec::with_capacity(xs.len()); for p in xs { v.push(p.read().await.clone()); } v }";
        assert!(run_on(src).is_empty());
    }

    // A bare awaited push value (no trailing transform) is still awaited.
    #[test]
    fn allows_simple_awaited_push_argument_issue_7843() {
        let src = "async fn f(xs: &[T]) -> Vec<U> { let mut v = Vec::new(); for p in xs { v.push(p.read().await); } v }";
        assert!(run_on(src).is_empty());
    }

    // Negative control: a synchronous `.clone()` transform has no `.await`, so
    // the `extend(...map(...))` rewrite is valid and the loop still flags.
    #[test]
    fn flags_sync_clone_transform_issue_7843() {
        let src = "fn f(src: Vec<u32>) { let mut dst = Vec::new(); for x in src { dst.push(x.clone()); } }";
        assert_eq!(run_on(src).len(), 1);
    }
}
