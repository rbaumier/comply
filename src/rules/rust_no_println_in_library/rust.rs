//! rust-no-println-in-library backend.
//!
//! Walks the tree for every `print!` / `println!` / `eprint!` /
//! `eprintln!` macro invocation and emits a diagnostic. The rule is
//! **context-aware**: it skips files that live in a pure binary crate
//! (no `src/lib.rs`, only `src/main.rs` and/or `[[bin]]` targets) since
//! writing to stdout is literally the point of a CLI binary. Mixed
//! workspaces (crates with both a `lib` and `bin` target) still get
//! linted — the heuristic is "does this crate have a library interface
//! that downstream code consumes?".
//!
//! Why not delegate to clippy::print_stdout? Clippy's lint fires on every
//! file in every workspace without knowing whether the crate is a lib or
//! a bin. Delegation worked out of the box but produced a 24-violation
//! false-positive storm on the comply project itself, which is a pure
//! bin crate. The custom check below is ~60 lines and does the right
//! thing by default.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        // Short-circuit: if the file lives in a pure bin crate, `println!`
        // is legitimate and we have nothing to say.
        if is_bin_only_crate(ctx.path) {
            return Vec::new();
        }
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "macro_invocation" {
                return;
            }
            let Some(macro_name_node) = node.child_by_field_name("macro") else {
                return;
            };
            let Ok(name) = macro_name_node.utf8_text(source_bytes) else {
                return;
            };
            let bare = name.rsplit("::").next().unwrap_or(name);
            if !matches!(bare, "print" | "println" | "eprint" | "eprintln") {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-no-println-in-library".into(),
                message: format!(
                    "`{bare}!` writes to stdout/stderr directly — library consumers \
                     can't redirect it, configure verbosity, or capture it in tests. \
                     Use `tracing::info!` / `tracing::debug!` with structured fields \
                     instead; the subscriber is the consumer's responsibility."
                ),
                severity: Severity::Error,
                span: None,
            });
        });
        diagnostics
    }
}

/// True if the crate containing `path` is a pure binary crate — no
/// `src/lib.rs` at the Cargo root. We walk up from `path` to find the
/// nearest `Cargo.toml`, then check whether `src/lib.rs` exists. Results
/// are cached per-workspace so repeated calls don't hit the filesystem.
fn is_bin_only_crate(path: &Path) -> bool {
    let Some(root) = find_cargo_root(path) else {
        return false; // Loose file — err on the side of linting.
    };
    static CACHE: Mutex<Option<Vec<(std::path::PathBuf, bool)>>> = Mutex::new(None);
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(Vec::new);
    if let Some((_, cached)) = cache.iter().find(|(r, _)| r == &root) {
        return *cached;
    }
    let has_lib_rs = root.join("src").join("lib.rs").is_file();
    let is_bin_only = !has_lib_rs;
    cache.push((root, is_bin_only));
    is_bin_only
}

fn find_cargo_root(path: &Path) -> Option<std::path::PathBuf> {
    let canonical = path.canonicalize().ok()?;
    let mut current = canonical.parent();
    while let Some(dir) = current {
        if dir.join("Cargo.toml").is_file() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str, path: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new(path), source), &tree)
    }

    #[test]
    fn flags_println_in_library_file() {
        // Synthesize a fake library path so `is_bin_only_crate` returns
        // false — canonicalize fails, find_cargo_root returns None, and
        // is_bin_only_crate returns false, so the rule fires.
        let source = "fn f() { println!(\"hello\"); }";
        let diags = run_on(source, "/nonexistent/lib.rs");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "rust-no-println-in-library");
    }

    #[test]
    fn flags_eprintln() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(run_on(source, "/nonexistent/lib.rs").len(), 1);
    }

    #[test]
    fn flags_print_and_eprint() {
        assert_eq!(run_on("fn f() { print!(\"x\"); }", "/nonexistent/lib.rs").len(), 1);
        assert_eq!(run_on("fn f() { eprint!(\"x\"); }", "/nonexistent/lib.rs").len(), 1);
    }

    #[test]
    fn allows_other_macros() {
        let source = "fn f() { vec![1, 2, 3]; format!(\"{}\", 1); }";
        assert!(run_on(source, "/nonexistent/lib.rs").is_empty());
    }
}
