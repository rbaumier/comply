//! rust-format-args-in-log-macro backend.
//!
//! For each log/tracing macro_invocation (`info!`, `debug!`, `warn!`,
//! `error!`, `trace!`), scan the token tree of the macro arguments for a
//! nested `format!(...)` macro_invocation. tree-sitter-rust parses the
//! tokens inside a macro invocation as a `token_tree`, and any nested
//! macro call appears as a `macro_invocation` descendant.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["macro_invocation"];

const LOG_MACROS: &[&str] = &["info", "debug", "warn", "error", "trace"];

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
        let Some(macro_name_node) = node.child_by_field_name("macro") else {
            return;
        };
        let Ok(macro_name) = macro_name_node.utf8_text(source) else {
            return;
        };
        // Last segment for `tracing::info!` / `log::info!` style.
        let last_segment = macro_name.rsplit("::").next().unwrap_or(macro_name);
        if !LOG_MACROS.contains(&last_segment) {
            return;
        }
        if !contains_format_macro(node, source) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "rust-format-args-in-log-macro",
            format!(
                "`{last_segment}!(\"{{}}\", format!(...))` double-formats. \
                 Pass the format args directly to `{last_segment}!` — log \
                 macros accept the same grammar as `format!`."
            ),
            Severity::Warning,
        ));
    }
}

/// True if the macro's token tree contains a nested `format!(` call.
/// tree-sitter-rust treats macro bodies as opaque `token_tree` nodes,
/// so we fall back to a text search on the token_tree content.
fn contains_format_macro(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "token_tree"
            && let Ok(text) = child.utf8_text(source)
                && text.contains("format!") {
                    return true;
                }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_info_with_inner_format() {
        let src = r#"fn f() { info!("{}", format!("x={}", 1)); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_tracing_warn_with_inner_format() {
        let src = r#"fn f() { tracing::warn!("{}", format!("oops {}", e)); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_error_with_inner_format() {
        let src = r#"fn f() { error!("err: {}", format!("{:?}", e)); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_plain_info() {
        let src = r#"fn f() { info!("x = {}", 1); }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_format_outside_log() {
        let src = r#"fn f() { let s = format!("x = {}", 1); }"#;
        assert!(run_on(src).is_empty());
    }
}
