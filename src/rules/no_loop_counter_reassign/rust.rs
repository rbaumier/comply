//! no-loop-counter-reassign Rust backend.
//!
//! Flag reassignment of loop counter inside a `while` loop body in Rust.
//! Rust `for` loops use immutable bindings, so only `while` is relevant.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "while_expression" {
        return;
    }

    let Some(condition) = node.child_by_field_name("condition") else { return };
    let Ok(cond_text) = condition.utf8_text(source) else { return };

    // Extract the variable from the condition (e.g., `i < n` -> `i`).
    let var_name = cond_text.split_whitespace().next().unwrap_or("");
    if var_name.is_empty() || !var_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return;
    }

    let Some(body) = node.child_by_field_name("body") else { return };
    let Ok(body_text) = body.utf8_text(source) else { return };

    // Check for reassignment patterns (not just += 1 which is the update).
    // Flag `var = <expr>` that isn't the typical counter update.
    // Split on `;` to handle single-line bodies.
    let assign_pattern = format!("{var_name} = ");
    let compound_pattern = format!("{var_name} += ");

    let mut non_update_assigns = 0;
    for stmt in body_text.split(';') {
        let trimmed = stmt.trim().trim_start_matches('{').trim();
        if trimmed.starts_with(&assign_pattern) && !trimmed.starts_with(&compound_pattern) {
            non_update_assigns += 1;
        }
    }

    if non_update_assigns > 0 {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-loop-counter-reassign".into(),
            message: format!("Loop counter `{var_name}` is reassigned inside the loop body."),
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
    fn flags_counter_reassign() {
        let src = "fn f() { let mut i = 0; while i < 10 { i = 5; i += 1; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_normal_update() {
        let src = "fn f() { let mut i = 0; while i < 10 { println!(\"{i}\"); i += 1; } }";
        assert!(run_on(src).is_empty());
    }
}
