//! intermediate-variables TypeScript / JavaScript / TSX backend.
//!
//! Flags `if` conditions that chain three or more boolean operands via
//! `&&` / `||` / `??`. See the crate-level docblock in `mod.rs`.

use crate::diagnostic::{Diagnostic, Severity};

const LOGICAL_OPS: &[&str] = &["&&", "||", "??"];
const CALLABLE_BOUNDARIES: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function",
    "generator_function_declaration",
];

fn count_logical_ops(node: tree_sitter::Node, source: &[u8]) -> usize {
    let mut count = 0;
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if CALLABLE_BOUNDARIES.contains(&current.kind()) {
            continue;
        }
        if current.kind() == "binary_expression"
            && let Some(op) = current.child_by_field_name("operator")
            && let Ok(op_text) = op.utf8_text(source)
            && LOGICAL_OPS.contains(&op_text)
        {
            count += 1;
        }
        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    count
}

crate::ast_check! { on ["if_statement"] => |node, source, ctx, diagnostics|
    let Some(condition) = node.child_by_field_name("condition") else { return };
    let min_ops = ctx.config.threshold("intermediate-variables", "min_ops");
    if count_logical_ops(condition, source) < min_ops {
        return;
    }
    let pos = condition.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "intermediate-variables".into(),
        message: "`if` condition chains three or more boolean operands \u{2014} extract parts into named local variables.".into(),
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
    fn flags_three_operand_and_chain() {
        assert_eq!(run_on("if (a && b && c) { x(); }").len(), 1);
    }

    #[test]
    fn flags_four_operand_or_chain() {
        assert_eq!(run_on("if (a || b || c || d) { x(); }").len(), 1);
    }

    #[test]
    fn flags_nullish_coalesce_chain() {
        assert_eq!(run_on("if (a ?? b ?? c) { x(); }").len(), 1);
    }

    #[test]
    fn allows_two_operand_and() {
        assert!(run_on("if (a && b) { x(); }").is_empty());
    }

    #[test]
    fn allows_single_condition() {
        assert!(run_on("if (a) { x(); }").is_empty());
    }

    #[test]
    fn allows_condition_with_comparisons_only() {
        assert!(run_on("if (a === 1 && b === 2) { x(); }").is_empty());
    }

    #[test]
    fn allows_long_expression_inside_comparison_chain() {
        assert!(run_on("if (a + b * c / d === e) { x(); }").is_empty());
    }

    #[test]
    fn does_not_flag_call_with_complex_arg() {
        // The outer node is a call_expression; the rule never inspects
        // calls at all, so complex arguments don't matter.
        assert!(run_on("doSomething(a + b * c / d);").is_empty());
    }

    #[test]
    fn closure_predicate_inside_condition_does_not_count() {
        // `.some(x => x.a && x.b && x.c)` is a lambda inside the call
        // argument. The walk stops at the arrow_function so its
        // operators don't contribute to the enclosing if's count.
        let src = "if (items.some(x => x.a && x.b && x.c && x.d)) { go(); }";
        assert!(run_on(src).is_empty());
    }
}
