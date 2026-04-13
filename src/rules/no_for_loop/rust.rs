//! no-for-loop Rust backend.
//!
//! Flag `while` loops with manual index that could be `for item in iter`.
//! Rust doesn't have C-style `for` loops, but `while i < len { ... i += 1 }`
//! is the equivalent anti-pattern.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "while_expression" {
        return;
    }

    let Some(condition) = node.child_by_field_name("condition") else { return };
    let Ok(cond_text) = condition.utf8_text(source) else { return };

    // Heuristic: `i < something.len()` or `i < N`.
    if !cond_text.contains(".len()") && !cond_text.contains("< ") {
        return;
    }

    // Check the body for `i += 1` pattern.
    let Some(body) = node.child_by_field_name("body") else { return };
    let Ok(body_text) = body.utf8_text(source) else { return };

    if body_text.contains("+= 1") || body_text.contains("= i + 1") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-for-loop".into(),
            message: "Manual index loop — use `for item in collection` or `.iter().enumerate()`.".into(),
            severity: Severity::Warning,
            span: None,
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
    fn flags_manual_index_loop() {
        let src = "fn f(v: &[i32]) { let mut i = 0; while i < v.len() { println!(\"{}\", v[i]); i += 1; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_for_in() {
        let src = "fn f(v: &[i32]) { for item in v { println!(\"{item}\"); } }";
        assert!(run_on(src).is_empty());
    }
}
