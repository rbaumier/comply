//! prefer-simple-condition-first backend — flag `complex && simple` where
//! the simple operand should come first for short-circuit optimization.
//!
//! "Simple" means:
//! - A bare identifier (`foo`)
//! - `!identifier`
//! - A binary `===`/`!==` where both sides are identifiers or literals
//! - A chain of all-simple conditions (e.g. `a && b`)
//!
//! Only flags in boolean contexts (if/while/do-while/for conditions,
//! ternary test, `!` operand) to avoid changing the produced value.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if the node represents a "simple" operand.
fn is_simple(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "identifier" | "true" | "false" | "null" | "undefined" => true,
        "number" | "string" => true,
        // !expr — simple if inner is simple
        "unary_expression" => {
            let op = node.child_by_field_name("operator")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");
            if op == "!" {
                node.child_by_field_name("argument")
                    .is_some_and(|arg| is_simple(arg, source))
            } else {
                // +N or -N where N is a number
                (op == "+" || op == "-")
                    && node.child_by_field_name("argument")
                        .is_some_and(|arg| arg.kind() == "number")
            }
        }
        // a === b or a !== b where both sides are simple operands
        "binary_expression" => {
            let op = node.child_by_field_name("operator")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");
            if op != "===" && op != "!==" {
                return false;
            }
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");
            let left_simple = left.is_some_and(|n| is_simple_operand(n, source));
            let right_simple = right.is_some_and(|n| is_simple_operand(n, source));
            // At least one side must be an identifier
            let has_ident = left.is_some_and(|n| n.kind() == "identifier")
                || right.is_some_and(|n| n.kind() == "identifier");
            left_simple && right_simple && has_ident
        }
        // A chain of all-simple conditions (prevents fix oscillation)
        "parenthesized_expression" => {
            node.named_child(0).is_some_and(|inner| is_simple(inner, source))
        }
        _ => false,
    }
}

/// Simple operand for binary comparison: identifier, literal, or signed number.
fn is_simple_operand(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "identifier" | "true" | "false" | "null" | "undefined" | "number" | "string" => true,
        "unary_expression" => {
            let op = node.child_by_field_name("operator")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");
            (op == "+" || op == "-")
                && node.child_by_field_name("argument")
                    .is_some_and(|arg| arg.kind() == "number")
        }
        _ => false,
    }
}

/// Check if a node has side effects (calls, member access, assignments, etc.)
#[allow(clippy::only_used_in_recursion)]
fn has_side_effects(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "call_expression" | "new_expression" | "assignment_expression"
        | "augmented_assignment_expression" | "update_expression"
        | "member_expression" | "await_expression" | "yield_expression" => true,
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

/// Check if the logical expression is used in a boolean context.
fn is_boolean_context(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        "if_statement" | "while_statement" | "do_statement" | "for_statement" => {
            // Must be the condition, not the body
            parent
                .child_by_field_name("condition")
                .is_some_and(|cond| cond.id() == node.id())
        }
        "ternary_expression" => {
            parent
                .child_by_field_name("condition")
                .is_some_and(|cond| cond.id() == node.id())
        }
        "unary_expression" => {
            let op = parent.child_by_field_name("operator")
                .and_then(|n| n.utf8_text(&[]).ok())
                .unwrap_or("");
            op == "!"
        }
        // Nested logical inherits context
        "binary_expression" if parent.kind() == "binary_expression" => {
            is_boolean_context(parent)
        }
        "parenthesized_expression" => is_boolean_context(parent),
        _ => false,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "binary_expression" {
        return;
    }

    let op = node.child_by_field_name("operator")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("");

    if op != "&&" && op != "||" {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    // Right must be simple, left must NOT be simple
    if !is_simple(right, source) || is_simple(left, source) {
        return;
    }

    // Only flag in boolean contexts
    if !is_boolean_context(node) {
        return;
    }

    // Skip if left has side effects (reordering would change semantics)
    if has_side_effects(left, source) {
        return;
    }

    let pos = right.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-simple-condition-first".into(),
        message: format!(
            "Prefer simple condition first in `{op}` expression."
        ),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_complex_before_simple() {
        let _src = "if (foo(x) && simple) { bar(); }";
        // foo(x) is a call (side effect) so actually skip
        // Let's use a ternary instead
        let src2 = "if ((a ? b : c) && simple) { bar(); }";
        let d = run_on(src2);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-simple-condition-first");
    }

    #[test]
    fn allows_simple_before_complex() {
        let src = "if (simple && (a ? b : c)) { bar(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_both_simple() {
        let src = "if (a && b) { foo(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn skips_side_effects() {
        // Member access has side effects, should not flag
        let src = "if (obj.method && simple) { foo(); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn skips_non_boolean_context() {
        // Not in a boolean context (value-producing)
        let src = "const x = (a ? b : c) && simple;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_or_operator() {
        let src = "if ((a ? b : c) || simple) { bar(); }";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }
}
