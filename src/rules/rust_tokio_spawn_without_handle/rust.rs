//! rust-tokio-spawn-without-handle backend.
//!
//! Walks `expression_statement` nodes and flags any whose direct
//! child is a `call_expression` to `tokio::spawn` (or any path
//! ending in `::spawn` from the tokio family). The fact that the
//! call sits at expression-statement level means its return value
//! — the JoinHandle — is being dropped, which is the bug.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;

const KINDS: &[&str] = &["expression_statement"];

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
        // The expression-statement wraps a single expression.
        // For `foo();` that's the `call_expression`.
        let Some(call) = node.named_child(0) else {
            return;
        };
        if call.kind() != "call_expression" {
            return;
        }
        let Some(function) = call.child_by_field_name("function") else {
            return;
        };
        let Ok(text) = function.utf8_text(source_bytes) else {
            return;
        };
        if !is_tokio_spawn(text) {
            return;
        }
        if is_in_test_context(node, source_bytes) {
            return;
        }
        let pos = call.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-tokio-spawn-without-handle".into(),
            message: format!(
                "`{text}(..)` discards its `JoinHandle` — panics in \
                 the spawned task are silently swallowed. Capture the \
                 handle and `.await` it, or wrap the work in a \
                 logging helper."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True if `text` looks like a `tokio::spawn` call by suffix match.
/// Accepts `tokio::spawn`, `tokio::task::spawn`, and bare `spawn`
/// (when paired with a `use tokio::spawn`).
fn is_tokio_spawn(text: &str) -> bool {
    text == "tokio::spawn"
        || text == "tokio::task::spawn"
        || text.ends_with("::tokio::spawn")
        || text == "spawn"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_fire_and_forget_spawn() {
        let source = "fn f() { tokio::spawn(async { work().await }); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_spawn_assigned_to_variable() {
        let source = "fn f() { let h = tokio::spawn(async { work().await }); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_spawn_returned() {
        let source = "fn f() -> JoinHandle<()> { tokio::spawn(async { work().await }) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_other_calls() {
        let source = "fn f() { other_function(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_spawn_in_tokio_test() {
        let source = r#"
#[tokio::test]
async fn my_test() {
    tokio::spawn(async move { work().await });
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_spawn_in_plain_test() {
        let source = r#"
#[test]
fn my_test() {
    tokio::spawn(async { work().await });
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_spawn_in_cfg_test_module() {
        let source = r#"
#[cfg(test)]
mod tests {
    fn helper() {
        tokio::spawn(async { work().await });
    }
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_spawn_in_production_code() {
        let source = r#"
fn start_worker() {
    tokio::spawn(async { process().await });
}
"#;
        assert_eq!(run_on(source).len(), 1);
    }
}
