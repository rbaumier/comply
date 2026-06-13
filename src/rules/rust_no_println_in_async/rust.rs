//! Detection: `macro_invocation` whose macro name is `println`,
//! `eprintln`, `print` or `eprint`, located inside async code — either an
//! `async fn` or an `async { … }` / `async move { … }` block.
//!
//! Source files of a binary-only crate (the nearest `Cargo.toml` declares
//! no `[lib]` table and no `src/lib.rs` exists next to it) are exempt:
//! the application owns its stdout, and interactive CLI prompts via
//! `print!` / `println!` are the feature there. The rule's concern is
//! library async code grabbing the application's stdout.

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

    if ctx.project.nearest_cargo_manifest(ctx.path).is_some_and(|m| m.is_binary_only()) { return; }

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
    use std::fs;
    use tempfile::TempDir;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    /// Run on `rel_path` inside a temp crate with the given `Cargo.toml`,
    /// so the crate-shape check resolves against a controlled manifest
    /// instead of comply's own (binary-only) `Cargo.toml`.
    fn run_in_crate(cargo_toml_contents: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml_contents).unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        let src_path = dir.path().join(rel_path);
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
    }

    const LIB_CARGO_TOML: &str = r#"
[package]
name = "mylib"
version = "0.1.0"
edition = "2021"

[lib]
name = "mylib"
path = "src/lib.rs"
"#;

    const BIN_ONLY_CARGO_TOML: &str = r#"
[package]
name = "mytool"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "mytool"
path = "src/main.rs"
"#;

    #[test]
    fn flags_println_in_async_fn() {
        let src = "async fn f() { println!(\"hi\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/service.rs", src).len(), 1);
    }

    #[test]
    fn flags_eprintln_in_async_fn() {
        let src = "async fn f() { eprintln!(\"err\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/service.rs", src).len(), 1);
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
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/service.rs", src).len(), 1);
    }

    #[test]
    fn flags_println_in_async_move_block() {
        let src = "fn f() { let _ = async move { println!(\"hi\"); }; }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/service.rs", src).len(), 1);
    }

    /// Regression for #980: in a binary-only crate (no `[lib]`, no
    /// `src/lib.rs`), `print!` in async code is an interactive CLI
    /// prompt — the application owns its stdout.
    #[test]
    fn allows_print_prompt_in_async_fn_binary_only_crate() {
        let src = "async fn do_backup() { print!(\"Enter Backup Code: \"); }";
        assert!(run_in_crate(BIN_ONLY_CARGO_TOML, "src/session.rs", src).is_empty());
    }

    #[test]
    fn flags_println_in_async_fn_in_library_crate_module() {
        let src = "async fn f() { println!(\"hi\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/util.rs", src).len(), 1);
    }
}
