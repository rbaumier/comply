//! Detection: `macro_invocation` whose macro name is `println`,
//! `eprintln`, `print` or `eprint`, located inside async code — either an
//! `async fn` or an `async { … }` / `async move { … }` block.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{is_in_test_context, is_inside_async_fn};

/// True when `node` lies inside an `async { … }` or `async move { … }` block.
/// tree-sitter-rust represents these as `async_block` nodes.
fn is_inside_async_block(node: tree_sitter::Node<'_>) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        if p.kind() == "async_block" {
            return true;
        }
        // Stop at function boundaries — `is_inside_async_fn` covers those.
        if p.kind() == "function_item" {
            return false;
        }
        cur = p.parent();
    }
    false
}

crate::ast_check! { on ["macro_invocation"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }
    if is_in_test_context(node, source) { return; }
    if ctx.path.to_string_lossy().contains("/examples/") { return; }

    let Some(macro_node) = node.child_by_field_name("macro") else { return; };
    let Ok(macro_name) = macro_node.utf8_text(source) else { return; };

    // Accept either the bare name or a path ending in the name
    // (`std::println!`, `::std::eprintln!`).
    let leaf = macro_name.rsplit("::").next().unwrap_or(macro_name);
    if !matches!(leaf, "println" | "eprintln" | "print" | "eprint") {
        return;
    }

    if !is_inside_async_fn(node, source) && !is_inside_async_block(node) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`{leaf}!` inside async code takes a blocking stdout/stderr lock. \
             Use `tracing::info!` / `tracing::warn!` instead — non-blocking, \
             filterable, span-aware."
        ),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_println_in_async_fn() {
        let src = "async fn f() { println!(\"hi\"); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_eprintln_in_async_fn() {
        let src = "async fn f() { eprintln!(\"err\"); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_println_in_sync_fn() {
        let src = "fn f() { println!(\"hi\"); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_tracing_info_in_async_fn() {
        let src = "async fn f() { tracing::info!(\"hi\"); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_println_in_async_block() {
        let src = "fn f() { let _ = async { println!(\"hi\"); }; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_println_in_async_move_block() {
        let src = "fn f() { let _ = async move { println!(\"hi\"); }; }";
        assert_eq!(run(src).len(), 1);
    }
}
