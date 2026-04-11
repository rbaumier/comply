//! error-without-cause Rust backend.
//!
//! Flags patterns like `anyhow!("{}", e.to_string())` or creating new errors
//! from `.to_string()` without preserving the source via `.context()` or
//! `.source()`. In Rust the idiomatic pattern is `.context("msg")` or
//! wrapping with `#[from]`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "macro_invocation" {
        return;
    }

    let Some(mac) = node.child_by_field_name("macro") else { return };
    let Ok(mac_name) = mac.utf8_text(source) else { return };

    if mac_name != "anyhow" && mac_name != "bail" {
        return;
    }

    let Ok(full_text) = node.utf8_text(source) else { return };

    // Check if the macro arguments reference `.to_string()` or `.message`
    // without also passing `source` or using `.context()`.
    // Also check surrounding context (e.g., `.context()` chained after the macro).
    let parent_text = node
        .parent()
        .and_then(|p| p.utf8_text(source).ok())
        .unwrap_or("");
    let combined = if parent_text.is_empty() { full_text } else { parent_text };

    if (full_text.contains(".to_string()") || full_text.contains(".message"))
        && !combined.contains("source")
        && !combined.contains("context")
        && !combined.contains("cause")
    {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "error-without-cause".into(),
            message: "Error wraps message without preserving cause — use `.context()` or pass `source`.".into(),
            severity: Severity::Error,
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
    fn flags_anyhow_with_to_string() {
        let src = r#"fn f(e: Error) { anyhow!("{}", e.to_string()); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_anyhow_with_context() {
        let src = r#"fn f(e: Error) { anyhow!("{}", e.to_string()).context("wrapping"); }"#;
        assert!(run_on(src).is_empty());
    }
}
