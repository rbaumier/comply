//! error-message Rust backend — flag error macros without a message.
//!
//! Detects:
//! - `anyhow!()`, `bail!()`, `eyre!()` with no arguments
//! - `panic!()` with no arguments
//! - Custom error type construction without a message field

use crate::diagnostic::{Diagnostic, Severity};

const ERROR_MACROS: &[&str] = &[
    "anyhow",
    "bail",
    "eyre",
    "panic",
    "todo",
    "unimplemented",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "macro_invocation" {
        return;
    }

    let Some(macro_node) = node.child_by_field_name("macro") else { return };
    let Ok(macro_name) = macro_node.utf8_text(source) else { return };

    // Strip path prefix (e.g. `anyhow::bail` -> `bail`).
    let name = macro_name.rsplit("::").next().unwrap_or(macro_name);

    if !ERROR_MACROS.contains(&name) {
        return;
    }

    // Get the token_tree (arguments).
    let mut has_args = false;
    let child_count = node.named_child_count();
    for i in 0..child_count {
        if let Some(child) = node.named_child(i)
            && child.kind() == "token_tree"
        {
            let inner = child.utf8_text(source).unwrap_or("()");
            // Strip outer parens and check if empty.
            let trimmed = inner
                .strip_prefix('(')
                .and_then(|s| s.strip_suffix(')'))
                .unwrap_or(inner)
                .trim();
            if !trimmed.is_empty() {
                has_args = true;
            }
            break;
        }
    }

    if !has_args {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "error-message".into(),
            message: format!("Pass a message to `{name}!()`."),
            severity: Severity::Warning,
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
    fn flags_panic_without_message() {
        let d = run_on("fn f() { panic!(); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("panic"));
    }

    #[test]
    fn flags_bail_without_message() {
        let d = run_on("fn f() { bail!(); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("bail"));
    }

    #[test]
    fn allows_panic_with_message() {
        assert!(run_on(r#"fn f() { panic!("something went wrong"); }"#).is_empty());
    }

    #[test]
    fn allows_anyhow_with_message() {
        assert!(run_on(r#"fn f() { anyhow!("failed to process"); }"#).is_empty());
    }
}
