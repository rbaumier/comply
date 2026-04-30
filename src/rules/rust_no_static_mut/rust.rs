//! rust-no-static-mut backend.
//!
//! Flags `static mut FOO: T = ...` declarations. The Rust 2024
//! edition deprecates this feature because every read or write
//! requires `unsafe` and there's no race-free path to use it
//! correctly without wrapping in a sync primitive — at which point
//! you might as well use the sync primitive directly.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["static_item"];

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
        // tree-sitter-rust represents `static mut FOO` by including
        // a `mutable_specifier` child holding the `mut` keyword.
        let mut cursor = node.walk();
        let has_mut = node
            .children(&mut cursor)
            .any(|c| c.kind() == "mutable_specifier");
        if !has_mut {
            return;
        }
        // Surface the static's name in the message if we can read it.
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source_bytes).ok())
            .unwrap_or("FOO");
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-static-mut".into(),
            message: format!(
                "`static mut {name}` — deprecated in Rust 2024 and \
                 impossible to use race-free. Use `OnceLock`/`LazyLock` \
                 for init-once, `Mutex`/`RwLock` for shared state, or \
                 `Atomic*` for primitive counters."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_static_mut() {
        assert_eq!(run_on("static mut COUNTER: u64 = 0;").len(), 1);
    }

    #[test]
    fn allows_static_immutable() {
        assert!(run_on("static MAX: u32 = 100;").is_empty());
    }

    #[test]
    fn allows_const() {
        assert!(run_on("const MAX: u32 = 100;").is_empty());
    }
}
