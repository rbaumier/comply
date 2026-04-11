//! no-misplaced-loop-counter Rust backend.
//!
//! Flag `while` loops where the condition and the update use different
//! variables. Rust doesn't have C-style `for` loops.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "while_expression" {
        return;
    }

    let Some(condition) = node.child_by_field_name("condition") else { return };
    let Ok(cond_text) = condition.utf8_text(source) else { return };

    // Extract variable from condition (first identifier).
    let cond_var = cond_text
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .find(|s| !s.is_empty() && s.chars().next().is_some_and(|c| c.is_alphabetic()));
    let Some(cond_var) = cond_var else { return };

    let Some(body) = node.child_by_field_name("body") else { return };
    let Ok(body_text) = body.utf8_text(source) else { return };

    // Look for `var += 1` update patterns. Split on `;` for single-line bodies.
    let mut update_var = None;
    for stmt in body_text.split(';') {
        let trimmed = stmt.trim().trim_start_matches('{').trim().trim_end_matches('}').trim();
        if let Some(rest) = trimmed.strip_suffix("+= 1") {
            let var = rest.trim();
            if !var.is_empty() && var.chars().all(|c| c.is_alphanumeric() || c == '_') {
                update_var = Some(var.to_string());
                break;
            }
        }
    }

    let Some(update_var) = update_var else { return };

    if cond_var != update_var {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-misplaced-loop-counter".into(),
            message: format!(
                "Condition uses `{cond_var}` but update modifies `{update_var}`."
            ),
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
    fn flags_mismatched_vars() {
        let src = "fn f() { let mut i = 0; let mut j = 0; while i < 10 { j += 1; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_matching_vars() {
        let src = "fn f() { let mut i = 0; while i < 10 { i += 1; } }";
        assert!(run_on(src).is_empty());
    }
}
