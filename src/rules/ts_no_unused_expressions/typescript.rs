//! ts-no-unused-expressions backend — flag expression statements whose
//! value is discarded.
//!
//! Extends the ESLint core rule for TS by also allowing:
//! - Non-null assertions (`x!`) as expression statements
//! - Type assertions (`x as T`) as expression statements
//! - `satisfies` expressions
//!
//! Allowed expression-statements (not flagged):
//! - Assignments (`x = 1`, `x += 1`)
//! - Call expressions (`foo()`, `obj.method()`)
//! - `new` expressions
//! - `await` expressions
//! - `yield` expressions
//! - `delete` expressions
//! - `void` expressions
//! - Unary increment/decrement (`i++`, `--j`)
//! - Tagged template literals
//! - Short-circuit expressions where RHS has side-effects (`a && b()`, `a || b()`)

use crate::diagnostic::{Diagnostic, Severity};

/// Check if an expression node has side effects (i.e. is allowed as a statement).
fn has_side_effects(node: tree_sitter::Node) -> bool {
    match node.kind() {
        // Always side-effectful
        "call_expression" | "new_expression" | "await_expression"
        | "yield_expression" | "assignment_expression"
        | "augmented_assignment_expression" | "update_expression"
        | "delete_expression" | "void_expression"
        | "tagged_template_expression" => true,

        // TS non-null assertion: unwrap and check inner
        "non_null_expression" => {
            if let Some(inner) = node.child_by_field_name("expression").or_else(|| node.named_child(0)) {
                has_side_effects(inner)
            } else {
                false
            }
        }

        // TS `as` assertion or `satisfies`: unwrap and check inner
        "as_expression" | "satisfies_expression" => {
            if let Some(inner) = node.named_child(0) {
                has_side_effects(inner)
            } else {
                false
            }
        }

        // Short-circuit: allowed if RHS has side effects
        "binary_expression" => {
            // Check operator
            let mut cursor = node.walk();
            let mut op = "";
            for child in node.children(&mut cursor) {
                if child.kind() == "&&" || child.kind() == "||" || child.kind() == "??" {
                    op = child.kind();
                    break;
                }
            }
            if (op == "&&" || op == "||" || op == "??")
                && let Some(right) = node.child_by_field_name("right") {
                    return has_side_effects(right);
                }
            false
        }

        // Ternary: allowed if both branches have side effects
        "ternary_expression" => {
            if let (Some(cons), Some(alt)) = (
                node.child_by_field_name("consequence"),
                node.child_by_field_name("alternative"),
            ) {
                has_side_effects(cons) && has_side_effects(alt)
            } else {
                false
            }
        }

        // Comma / sequence: last expression matters
        "sequence_expression" => {
            let count = node.named_child_count();
            if count > 0
                && let Some(last) = node.named_child(count - 1) {
                    return has_side_effects(last);
                }
            false
        }

        // Parenthesized
        "parenthesized_expression" => {
            node.named_child(0).is_some_and(|c| has_side_effects(c))
        }

        _ => false,
    }
}

crate::ast_check! { on ["expression_statement"] => |node, source, ctx, diagnostics|
    // Get the expression child
    let Some(expr) = node.named_child(0) else {
        return;
    };

    // String literals in expression position are allowed (directive prologues like "use strict")
    if expr.kind() == "string" || expr.kind() == "template_string" {
        return;
    }

    if has_side_effects(expr) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-unused-expressions".into(),
        message: "Expected an assignment or function call, got an expression with no side effects.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_bare_identifier() {
        let d = run_on("let x = 1; x;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_function_call() {
        assert!(run_on("console.log('hello');").is_empty());
    }

    #[test]
    fn allows_assignment() {
        assert!(run_on("let x = 1; x = 2;").is_empty());
    }

    #[test]
    fn flags_bare_arithmetic() {
        let d = run_on("1 + 2;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_short_circuit_with_call() {
        assert!(run_on("const x = true; x && console.log('y');").is_empty());
    }
}
