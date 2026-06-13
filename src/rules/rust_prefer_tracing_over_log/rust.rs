//! rust-prefer-tracing-over-log backend.
//!
//! Two AST shapes flag this rule:
//!
//! 1. `use log::…` — `use_declaration` whose path begins with `log::`
//!    (matching `use log::info;`, `use log::{info, warn};`, etc).
//! 2. `log::info!` / `log::warn!` / `log::error!` / `log::debug!` /
//!    `log::trace!` — `macro_invocation` whose `macro` child resolves
//!    to a `scoped_identifier` rooted at `log`.
//!
//! Both shapes are detected via the leading-text check on the node.
//! tree-sitter-rust models `log::info!` as a `macro_invocation` with
//! a `scoped_identifier` macro path, so the textual prefix check
//! (`text.starts_with("log::")`) is the simplest correct match.
//!
//! ## Async-only exemption
//!
//! `tracing`'s key advantage over `log` is span context propagation across
//! `async` boundaries. In synchronous crates (no `tokio`, `async-std`, or
//! `futures` in the nearest `Cargo.toml`), `log` is the established standard
//! and switching would add a heavier dependency for no functional gain. The
//! rule is therefore silenced for crates that have no async runtime dependency.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["use_declaration", "macro_invocation"];

const LOG_MACROS: &[&str] = &["info", "warn", "error", "debug", "trace"];

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
        let hit = match node.kind() {
            "use_declaration" => is_log_use(node, source_bytes),
            "macro_invocation" => is_log_macro_call(node, source_bytes),
            _ => false,
        };
        if !hit {
            return;
        }
        // Silence the rule for crates that have no async runtime dependency.
        // `tracing`'s advantage (span context across `async` boundaries) does
        // not apply in purely synchronous code, where `log` is the established
        // standard with a smaller footprint.
        // Missing/unparseable Cargo.toml defaults to flagging (`None` -> `true`).
        if !ctx
            .project
            .nearest_cargo_manifest(ctx.path)
            .is_none_or(|m| m.has_async_runtime())
        {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-prefer-tracing-over-log".into(),
            message: "Prefer the `tracing` crate over `log`. `tracing` carries \
                      structured fields and span context across `async` \
                      boundaries; `log` does not."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_log_use(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else {
        return false;
    };
    let trimmed = text.trim_start();
    // Skip `pub` modifiers so `pub use log::info;` is also caught.
    let after_pub = trimmed
        .strip_prefix("pub(crate)")
        .or_else(|| trimmed.strip_prefix("pub(super)"))
        .or_else(|| trimmed.strip_prefix("pub"))
        .unwrap_or(trimmed)
        .trim_start();
    let Some(rest) = after_pub.strip_prefix("use") else {
        return false;
    };
    let path = rest.trim_start();
    // Any of: `log::…`, `log ;` (alias), or `log;` (shouldn't really
    // happen for the log crate but keep the check tight).
    path.starts_with("log::") || path.starts_with("log ") || path.starts_with("log;")
}

fn is_log_macro_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(macro_node) = node.child_by_field_name("macro") else {
        return false;
    };
    let Ok(name) = macro_node.utf8_text(source) else {
        return false;
    };
    // Match `log::info`, `log::warn`, `log::error`, `log::debug`, `log::trace`.
    let Some(suffix) = name.strip_prefix("log::") else {
        return false;
    };
    LOG_MACROS.contains(&suffix)
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

    /// Run on a path within comply's own worktree, which has `tokio` in
    /// its `Cargo.toml`. This ensures the "async runtime present" path is
    /// exercised by the basic positive/negative tests.
    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    /// Run on a file in `dir/src/t.rs` with the given `Cargo.toml` contents.
    fn run_on_with_cargo(cargo_toml_contents: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml_contents).unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        let src_path = dir.path().join("src/t.rs");
        fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
    }

    #[test]
    fn flags_use_log_single() {
        assert_eq!(run_on("use log::info;").len(), 1);
    }

    #[test]
    fn flags_use_log_group() {
        assert_eq!(run_on("use log::{info, warn};").len(), 1);
    }

    #[test]
    fn flags_log_info_macro() {
        assert_eq!(run_on(r#"fn f() { log::info!("hi"); }"#).len(), 1);
    }

    #[test]
    fn flags_log_warn_macro() {
        assert_eq!(run_on(r#"fn f() { log::warn!("hi"); }"#).len(), 1);
    }

    #[test]
    fn flags_log_error_macro() {
        assert_eq!(run_on(r#"fn f() { log::error!("hi"); }"#).len(), 1);
    }

    #[test]
    fn allows_use_tracing() {
        assert!(run_on("use tracing::info;").is_empty());
    }

    #[test]
    fn allows_tracing_macro() {
        assert!(run_on(r#"fn f() { tracing::info!("hi"); }"#).is_empty());
    }

    #[test]
    fn allows_unrelated_log_named_module() {
        // `mylog::info!` is not the `log` crate.
        assert!(run_on(r#"fn f() { mylog::info!("hi"); }"#).is_empty());
    }

    // ── Async-exemption regression tests (Closes #990) ──────────────────

    const SYNC_CARGO_TOML: &str = r#"
[package]
name = "searcher"
version = "0.1.0"
edition = "2021"

[dependencies]
log = "0.4"
"#;

    const ASYNC_CARGO_TOML: &str = r#"
[package]
name = "my-server"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
log = "0.4"
"#;

    /// Regression for #990: `log::trace!` in a synchronous crate (no tokio/
    /// async-std/futures) must not be flagged — switching to `tracing` would
    /// add a heavier dependency with no functional benefit.
    #[test]
    fn no_fp_on_sync_crate_log_macro() {
        let src = r#"fn f() { log::trace!("searcher core: will use fast line searcher"); }"#;
        assert!(
            run_on_with_cargo(SYNC_CARGO_TOML, src).is_empty(),
            "must not flag log::trace! in a synchronous crate"
        );
    }

    #[test]
    fn no_fp_on_sync_crate_log_use() {
        assert!(
            run_on_with_cargo(SYNC_CARGO_TOML, "use log::trace;").is_empty(),
            "must not flag `use log::…` in a synchronous crate"
        );
    }

    #[test]
    fn still_flags_log_macro_in_async_crate() {
        let src = r#"fn f() { log::info!("hello"); }"#;
        assert_eq!(
            run_on_with_cargo(ASYNC_CARGO_TOML, src).len(),
            1,
            "must flag log::info! when tokio is a dependency"
        );
    }

    #[test]
    fn no_cargo_toml_defaults_to_flagging() {
        // When no Cargo.toml is found, default to flagging (safe fallback).
        let src = r#"fn f() { log::warn!("fallback"); }"#;
        let diagnostics = crate::rules::test_helpers::run_rule(
            &Check,
            src,
            "/nonexistent_cargo_project/src/t.rs",
        );
        assert_eq!(
            diagnostics.len(),
            1,
            "must flag when Cargo.toml is absent (safe default)"
        );
    }
}
