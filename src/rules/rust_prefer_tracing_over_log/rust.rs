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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
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
}
