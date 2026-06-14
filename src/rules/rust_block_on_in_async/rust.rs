//! rust-block-on-in-async backend.
//!
//! Walks `call_expression` nodes whose function path ends in
//! `block_on` and verifies the call is inside an `async fn`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_inside_async_fn;
use tree_sitter::Node;

/// Combinators whose closure argument runs blocking code off the async
/// worker, so a `block_on` lexically inside it does not start a runtime
/// within the running runtime: `task::block_in_place` runs on the current
/// thread but tells tokio to move other tasks off it, and `thread::spawn`
/// (and `Builder::spawn`) runs on a separate OS thread with no runtime
/// context. Matched against the call's final path segment only.
const BLOCKING_OFFLOAD_COMBINATORS: &[&str] = &["block_in_place", "spawn"];

/// True if `node` is lexically inside a synchronous closure that is an
/// argument to one of the `BLOCKING_OFFLOAD_COMBINATORS`. Walks the
/// ancestor chain for the nearest enclosing `closure_expression`, then
/// confirms that closure sits in the `arguments` of a `call_expression`
/// whose `function`'s final segment is an offload combinator.
///
/// Only a `closure_expression` (`|| { … }` / `move || { … }`) qualifies.
/// `tokio::task::spawn` takes an `async_block`, not a closure, so a
/// `block_on` inside `task::spawn(async { … })` — which is still unsafe —
/// is never exempted here.
fn is_inside_blocking_offload_closure(node: Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "closure_expression"
            && let Some(args) = parent.parent()
            && args.kind() == "arguments"
            && let Some(call) = args.parent()
            && call.kind() == "call_expression"
            && let Some(function) = call.child_by_field_name("function")
            && let Some(segment) = call_final_segment(function, source)
            && BLOCKING_OFFLOAD_COMBINATORS.contains(&segment)
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// The final path segment of a `call_expression`'s `function` child: the
/// `name` field of a `scoped_identifier` (`std::thread::spawn` → `spawn`),
/// or the text of a bare `identifier` (`spawn` → `spawn`). Any other shape
/// (method call, parenthesised expression) yields `None`.
fn call_final_segment<'a>(function: Node, source: &'a [u8]) -> Option<&'a str> {
    let segment = match function.kind() {
        "scoped_identifier" => function.child_by_field_name("name")?,
        "identifier" => function,
        _ => return None,
    };
    segment.utf8_text(source).ok()
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(text) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !text.ends_with("block_on") {
        return;
    }
    if !is_inside_async_fn(node, source) {
        return;
    }
    if is_inside_blocking_offload_closure(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "rust-block-on-in-async".into(),
        message: format!(
            "`{text}(..)` from inside an `async fn` triggers tokio's `Cannot \
             start a runtime from within a runtime` panic. Use `.await` on the \
             future instead."
        ),
        severity: Severity::Error,
        span: None,
    });
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
    fn flags_block_on_inside_async() {
        let source = "async fn f() { rt.block_on(other_future()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_futures_block_on_inside_async() {
        let source = "async fn f() { futures::executor::block_on(other()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_block_on_in_main() {
        let source = "fn main() { rt.block_on(server()); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_block_on_inside_block_in_place_closure() {
        let source = "async fn f() { task::spawn(async { task::block_in_place(move || { \
                      rt.block_on(fut).unwrap(); }); }); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_free_block_on_inside_block_in_place_closure() {
        let source = "async fn f() { task::block_in_place(|| futures::executor::block_on(fut)); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_block_on_inside_thread_spawn_closure() {
        let source = "async fn f() { std::thread::spawn(move || { handle.block_on(fut) }); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_block_on_directly_in_async_fn_body() {
        let source = "async fn f() { rt.block_on(fut); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_block_on_inside_task_spawn_async_block() {
        let source = "async fn f() { task::spawn(async move { rt.block_on(fut); }); }";
        assert_eq!(run_on(source).len(), 1);
    }
}
