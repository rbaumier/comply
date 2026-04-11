//! no-pseudo-random backend for Rust.
//!
//! Flags `thread_rng()` and `random()` from the `rand` crate — use
//! `OsRng` or `rand::rngs::OsRng` for security-sensitive contexts.

use crate::diagnostic::{Diagnostic, Severity};

const INSECURE_FUNCTIONS: &[&str] = &["thread_rng", "random"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");

    // Match bare `thread_rng()`, `random()`, or qualified `rand::thread_rng()`, etc.
    for &func in INSECURE_FUNCTIONS {
        if callee_text == func || callee_text.ends_with(&format!("::{func}")) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-pseudo-random".into(),
                message: format!(
                    "`{callee_text}()` is not cryptographically secure — use `OsRng` or `rand::rngs::OsRng`.",
                ),
                severity: Severity::Warning,
            });
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_thread_rng() {
        assert_eq!(run_on("fn f() { let rng = thread_rng(); }").len(), 1);
    }

    #[test]
    fn flags_rand_random() {
        assert_eq!(run_on("fn f() { let x = rand::random(); }").len(), 1);
    }

    #[test]
    fn allows_os_rng() {
        assert!(run_on("fn f() { let rng = OsRng; }").is_empty());
    }
}
