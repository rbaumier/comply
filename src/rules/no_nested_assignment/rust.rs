//! no-nested-assignment Rust backend.
//!
//! Flag assignments inside conditions: `if (x = ...) {}`.
//! In Rust this is less common but can happen via `=` in conditions.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, _ctx, diagnostics|
    match node.kind() {
        "if_expression" | "while_expression" => {}
        _ => return,
    }
    let Some(condition) = node.child_by_field_name("condition") else {
        return;
    };
    if contains_assignment(condition) {
        let pos = condition.start_position();
        diagnostics.push(Diagnostic {
            path: _ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-nested-assignment".into(),
            message: "Assignment inside a condition \u{2014} likely a bug, use `==` for comparison or move the assignment out.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn contains_assignment(node: tree_sitter::Node) -> bool {
    if node.kind() == "assignment_expression" {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if contains_assignment(child) {
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
    fn allows_equality_check() {
        assert!(run_on("fn f() { if x == 10 {} }").is_empty());
    }

    #[test]
    fn allows_comparison() {
        assert!(run_on("fn f() { if x <= 10 {} }").is_empty());
    }
}
