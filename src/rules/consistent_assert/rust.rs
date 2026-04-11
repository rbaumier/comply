//! consistent-assert Rust backend.
//!
//! In Rust, flag bare `assert!(expr)` when `debug_assert!` would be more
//! appropriate, or flag inconsistent assert styles. For now, we flag
//! `assert!(x == y)` which should be `assert_eq!(x, y)` for better messages.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "macro_invocation" {
        return;
    }

    let Some(mac) = node.child_by_field_name("macro") else { return };
    let Ok(mac_name) = mac.utf8_text(source) else { return };

    if mac_name != "assert" {
        return;
    }

    // Get the token tree content.
    let Ok(full_text) = node.utf8_text(source) else { return };
    // Check for `assert!(x == y)` pattern.
    if full_text.contains("==") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "consistent-assert".into(),
            message: "Use `assert_eq!(a, b)` instead of `assert!(a == b)` for better error messages.".into(),
            severity: Severity::Warning,
        });
    }
    if full_text.contains("!=") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "consistent-assert".into(),
            message: "Use `assert_ne!(a, b)` instead of `assert!(a != b)` for better error messages.".into(),
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
    fn flags_assert_eq_pattern() {
        assert_eq!(run_on("fn f() { assert!(x == y); }").len(), 1);
    }

    #[test]
    fn flags_assert_ne_pattern() {
        assert_eq!(run_on("fn f() { assert!(x != y); }").len(), 1);
    }

    #[test]
    fn allows_assert_eq_macro() {
        assert!(run_on("fn f() { assert_eq!(x, y); }").is_empty());
    }

    #[test]
    fn allows_bare_assert() {
        assert!(run_on("fn f() { assert!(is_valid); }").is_empty());
    }
}
