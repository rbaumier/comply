//! rust-eprintln-in-library backend.
//!
//! Walks `macro_invocation` nodes for `eprintln!` / `eprint!` and
//! flags any invocation that:
//!
//! - is **not** in test context (`#[test]` / `#[cfg(test)]` /
//!   `tests/` integration directory), and
//! - is **not** in a binary file (`main.rs`, `src/bin/*.rs`), and
//! - is **not** in a crate that declares a binary (the nearest
//!   `Cargo.toml` declares a `[[bin]]` target or a `src/main.rs`
//!   exists next to it).
//!
//! `eprintln!` is fine in CLI binaries — that's where it belongs.
//! It's a problem in libraries because consumers can't redirect or
//! capture it. A crate that ships a binary is an application: every
//! one of its source files is exempt, not just the entry points —
//! even when it also carries a `[lib]` purely to expose internals to
//! its own integration tests (the `lib.rs` + `main.rs` split).

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
        if ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_some_and(|m| m.declares_binary())
        {
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

    fn run_on(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
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

    /// starship shape: a CLI binary that carries a `[lib]` table (and a
    /// `src/lib.rs`) purely to expose internals to its own integration tests,
    /// alongside a `[[bin]]` target that is the real entry point.
    /// `is_binary_only()` is false here, yet the crate still owns its stderr.
    const CLI_WITH_LIB_CARGO_TOML: &str = r#"
[package]
name = "starship"
version = "0.1.0"
edition = "2021"

[lib]
name = "starship"
path = "src/lib.rs"

[[bin]]
name = "starship"
path = "src/main.rs"
"#;

    #[test]
    fn flags_eprintln_in_library_file() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    #[test]
    fn flags_eprint_in_library_file() {
        let source = "fn f() { eprint!(\"oops\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/lib.rs", source).len(), 1);
    }

    /// Regression for #981: a module of a binary-only crate (no `[lib]`,
    /// no `src/lib.rs`) has no library consumers — `eprintln!` is fine
    /// even outside `main.rs` / `bin/`.
    #[test]
    fn allows_eprintln_in_binary_only_crate_module() {
        let source = "fn print_help() { eprintln!(\"usage\"); }";
        assert!(run_in_crate(BIN_ONLY_CARGO_TOML, "src/session.rs", source).is_empty());
    }

    #[test]
    fn flags_eprintln_in_library_crate_module() {
        let source = "fn f() { eprintln!(\"oops\"); }";
        assert_eq!(run_in_crate(LIB_CARGO_TOML, "src/util.rs", source).len(), 1);
    }

    /// Regression for #1312: starship declares a `[[bin]]` target (the
    /// `starship` CLI) alongside a `[lib]` used only to expose internals to
    /// integration tests. `eprintln!` in setup/logger code is controlled CLI
    /// error output — not a library writing to a consumer's stderr.
    #[test]
    fn allows_eprintln_in_cli_crate_with_internal_lib() {
        let source = "fn init_logger() { eprintln!(\"Unable to create log dir\"); }";
        assert!(run_in_crate(CLI_WITH_LIB_CARGO_TOML, "src/logger.rs", source).is_empty());
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
