//! rust-eprintln-in-library backend.
//!
//! Walks `macro_invocation` nodes for `eprintln!` / `eprint!` and
//! flags any invocation that:
//!
//! - is **not** in test context (`#[test]` / `#[cfg(test)]` /
//!   `tests/` integration directory), and
//! - is **not** in a binary file (`main.rs`, `src/bin/*.rs`).
//!
//! `eprintln!` is fine in CLI binaries — that's where it belongs.
//! It's a problem in libraries because consumers can't redirect or
//! capture it.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;
use std::path::Path;

const KINDS: &[&str] = &["macro_invocation"];

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
        let Some(macro_name) = node.child_by_field_name("macro") else {
            return;
        };
        let name = macro_name.utf8_text(source_bytes).unwrap_or("");
        let bare = name.rsplit("::").next().unwrap_or(name);
        if bare != "eprintln" && bare != "eprint" {
            return;
        }
        if is_in_test_context(node, source_bytes) || is_under_tests_dir(ctx.path) {
            return;
        }
        if is_binary_file(ctx.path) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            std::sync::Arc::clone(&ctx.path_arc),
            &node,
            "rust-eprintln-in-library",
            format!(
                "`{bare}!` writes to stderr directly — library consumers \
                 can't redirect, configure, or capture it. Use \
                 `tracing::warn!` / `tracing::error!` instead."
            ),
            Severity::Warning,
        ));
    }
}

fn is_under_tests_dir(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "tests")
}

/// True if `path` is a binary entry point: `main.rs` at any directory
/// level, or any file under a `bin/` directory.
fn is_binary_file(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str())
        && name == "main.rs"
    {
        return true;
    }
    path.components().any(|c| c.as_os_str() == "bin")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust_with_path(source, &Check, path)
    }

    #[test]
    fn flags_eprintln_in_library_file() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(run_on(source, "src/lib.rs").len(), 1);
    }

    #[test]
    fn flags_eprint_in_library_file() {
        let source = "fn f() { eprint!(\"oops\"); }";
        assert_eq!(run_on(source, "src/lib.rs").len(), 1);
    }

    #[test]
    fn allows_eprintln_in_main_rs() {
        let source = "fn main() { eprintln!(\"oops\"); }";
        assert!(run_on(source, "src/main.rs").is_empty());
    }

    #[test]
    fn allows_eprintln_in_bin_dir() {
        let source = "fn main() { eprintln!(\"oops\"); }";
        assert!(run_on(source, "src/bin/tool.rs").is_empty());
    }

    #[test]
    fn allows_eprintln_in_test_function() {
        let source = "#[test]\nfn t() { eprintln!(\"trace\"); }";
        assert!(run_on(source, "src/lib.rs").is_empty());
    }

    #[test]
    fn allows_eprintln_in_tests_dir() {
        let source = "fn f() { eprintln!(\"trace\"); }";
        assert!(run_on(source, "tests/it.rs").is_empty());
    }
}
