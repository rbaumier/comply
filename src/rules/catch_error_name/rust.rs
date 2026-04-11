//! catch-error-name Rust backend — flag match arm error bindings not named `error`.
//!
//! In Rust, `catch` blocks don't exist yet; the pattern is
//! `Err(e) =>` in match arms. We check that the binding is `error`, `_`, or
//! ends with `error`/`Error`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "match_arm" {
        return;
    }
    let Some(pattern) = node.child_by_field_name("pattern") else { return };
    let Ok(pat_text) = pattern.utf8_text(source) else { return };

    // Only check Err(...) patterns.
    if !pat_text.starts_with("Err(") {
        return;
    }

    // Extract the binding name inside Err(...).
    let inner = &pat_text[4..pat_text.len().saturating_sub(1)];
    let name = inner.trim();

    if name == "_" || name == "error" || name.ends_with("error") || name.ends_with("Error") || name == "err" || name.ends_with("_err") {
        return;
    }

    let pos = pattern.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "catch-error-name".into(),
        message: format!(
            "Error binding `{name}` should be named `error` (or `err`, `_`)."
        ),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_bad_error_name() {
        let src = r#"fn f() { match result { Err(e) => {}, Ok(v) => {} } }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_error_name() {
        let src = r#"fn f() { match result { Err(error) => {}, Ok(v) => {} } }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_underscore() {
        let src = r#"fn f() { match result { Err(_) => {}, Ok(v) => {} } }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_err() {
        let src = r#"fn f() { match result { Err(err) => {}, Ok(v) => {} } }"#;
        assert!(run_on(src).is_empty());
    }
}
