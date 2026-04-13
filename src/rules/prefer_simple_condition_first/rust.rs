//! prefer-simple-condition-first Rust backend — flag `complex && simple`
//! where the simple operand should come first for short-circuit optimization.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if the node represents a "simple" operand.
fn is_simple(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "identifier" | "boolean_literal" | "integer_literal" | "float_literal"
        | "string_literal" | "char_literal" => true,
        // !expr — simple if inner is simple
        // In tree-sitter-rust, unary_expression has no fields:
        // child(0) is the operator, named_child(0) is the operand.
        "unary_expression" => {
            let op = node
                .child(0)
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");
            if op == "!" {
                node.named_child(0)
                    .is_some_and(|arg| is_simple(arg, source))
            } else {
                // -N where N is a number
                op == "-"
                    && node
                        .named_child(0)
                        .is_some_and(|arg| {
                            matches!(arg.kind(), "integer_literal" | "float_literal")
                        })
            }
        }
        // a == b or a != b where both sides are simple
        "binary_expression" => {
            let op = node
                .child_by_field_name("operator")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");
            if op != "==" && op != "!=" {
                return false;
            }
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");
            let left_simple = left.is_some_and(|n| is_simple_operand(n));
            let right_simple = right.is_some_and(|n| is_simple_operand(n));
            let has_ident = left.is_some_and(|n| n.kind() == "identifier")
                || right.is_some_and(|n| n.kind() == "identifier");
            left_simple && right_simple && has_ident
        }
        "parenthesized_expression" => {
            node.named_child(0)
                .is_some_and(|inner| is_simple(inner, source))
        }
        _ => false,
    }
}

fn is_simple_operand(node: tree_sitter::Node) -> bool {
    matches!(
        node.kind(),
        "identifier"
            | "boolean_literal"
            | "integer_literal"
            | "float_literal"
            | "string_literal"
            | "char_literal"
    )
}

fn has_side_effects(node: tree_sitter::Node) -> bool {
    match node.kind() {
        "call_expression" | "macro_invocation" | "assignment_expression"
        | "compound_assignment_expr" | "await_expression" | "field_expression"
        | "method_call_expression" => true,
        _ => {
            let count = node.child_count();
            for i in 0..count {
                if let Some(child) = node.child(i)
                    && has_side_effects(child)
                {
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
        "if_expression" | "while_expression" => parent
            .child_by_field_name("condition")
            .is_some_and(|cond| cond.id() == node.id()),
        "unary_expression" => {
            let op = parent
                .child(0)
                .and_then(|n| n.utf8_text(&[]).ok())
                .unwrap_or("");
            op == "!"
        }
        "binary_expression" => is_boolean_context(parent),
        "parenthesized_expression" => is_boolean_context(parent),
        _ => false,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "binary_expression" {
        return;
    }

    let op = node
        .child_by_field_name("operator")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("");

    if op != "&&" && op != "||" {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    // Right must be simple, left must NOT be simple.
    if !is_simple(right, source) || is_simple(left, source) {
        return;
    }

    // Only flag in boolean contexts.
    if !is_boolean_context(node) {
        return;
    }

    // Skip if left has side effects (reordering would change semantics).
    if has_side_effects(left) {
        return;
    }

    let pos = right.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-simple-condition-first".into(),
        message: format!("Prefer simple condition first in `{op}` expression."),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_complex_before_simple() {
        // Use a ternary-like expression (match as condition proxy)
        let src = "fn f(a: bool, b: bool, c: bool) { if (if a { b } else { c }) && b { foo(); } }";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-simple-condition-first");
    }

    #[test]
    fn allows_both_simple() {
        let src = "fn f(a: bool, b: bool) { if a && b { foo(); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn skips_side_effects() {
        // Field access has side effects, should not flag.
        let src = "fn f(obj: Foo, simple: bool) { if obj.method && simple { foo(); } }";
        assert!(run_on(src).is_empty());
    }
}
