//! no-hidden-control-flow Rust backend — flag `&&` short-circuit with
//! side effects on the right side.
//!
//! In Rust, `x && side_effect()` uses short-circuit evaluation where
//! `side_effect()` only runs when `x` is true. This hides control flow.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "binary_expression" {
        return;
    }

    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let Ok(op) = op_node.utf8_text(source) else { return };
    if op != "&&" {
        return;
    }

    let Some(right) = node.child_by_field_name("right") else { return };

    // Check if the right side has side effects (call expressions, macros, etc.)
    if has_side_effects(right, source) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-hidden-control-flow".into(),
            message: "`&&` short-circuit hides control flow \u{2014} use `if` for clarity.".into(),
            severity: Severity::Warning,
        });
    }
}

#[allow(clippy::only_used_in_recursion)]
fn has_side_effects(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "call_expression" | "macro_invocation" | "assignment_expression"
        | "compound_assignment_expr" | "await_expression" => true,
        _ => {
            let count = node.child_count();
            for i in 0..count {
                if let Some(child) = node.child(i)
                    && has_side_effects(child, source) {
                        return true;
                }
            }
            false
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
    fn flags_short_circuit_with_call() {
        let d = run_on("fn f(x: bool) { x && do_something(); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("short-circuit"));
    }

    #[test]
    fn allows_simple_boolean_and() {
        assert!(run_on("fn f(a: bool, b: bool) -> bool { a && b }").is_empty());
    }

    #[test]
    fn allows_if_expression() {
        assert!(run_on("fn f(x: bool) { if x { do_something(); } }").is_empty());
    }
}
