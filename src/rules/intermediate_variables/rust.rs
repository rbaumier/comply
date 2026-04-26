//! intermediate-variables Rust backend.
//!
//! Flags `if` conditions that chain three or more boolean operands via
//! `&&` / `||`. The remediation is to extract some of the operands
//! into named local variables so that the `if` reads as one or two
//! higher-level checks rather than a flat conjunction of five things.
//!
//! Only the `condition` field of `if_expression` is walked, and the
//! walk stops at nested callables (`closure_expression`,
//! `function_item`) so that lambda predicates passed to combinators
//! (`.filter(|x| x.a && x.b && x.c)`) don't contribute to the
//! enclosing `if`'s operator count.

use crate::diagnostic::{Diagnostic, Severity};

const LOGICAL_OPS: &[&str] = &["&&", "||"];
const CALLABLE_BOUNDARIES: &[&str] = &["closure_expression", "function_item"];

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

crate::ast_check! { on ["if_expression"] => |node, source, ctx, diagnostics|
    let Some(condition) = node.child_by_field_name("condition") else { return };
    let min_ops = ctx.config.threshold("intermediate-variables", "min_ops");
    if count_logical_ops(condition, source) < min_ops {
        return;
    }
    let pos = condition.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
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
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_three_operand_and_chain() {
        let src = "fn f() { if a && b && c { x(); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_four_operand_or_chain() {
        let src = "fn f() { if a || b || c || d { x(); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_mixed_and_or() {
        let src = "fn f() { if a && b || c { x(); } }";
        // 1 && + 1 || = 2 logical ops → flag.
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_two_operand_and() {
        let src = "fn f() { if a && b { x(); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_condition() {
        let src = "fn f() { if a { x(); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_condition_with_comparisons_only() {
        // One `&&` plus a comparison `!=` — comparisons aren't logical ops,
        // so only 1 logical op in the chain → not flagged.
        let src = r#"
fn f() {
    if !output.status.success() && output.status.code() != Some(1) {
        do_stuff();
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_long_expression_inside_comparison_chain() {
        // Arithmetic and comparison ops do NOT contribute to the count.
        let src = "fn f() { if a + b * c / d == e { x(); } }";
        assert!(run_on(src).is_empty());
    }

    /// Regression for the walkthrough FP: the outer node was a
    /// `call_expression` (`walk_tree(..., |node| { ... })`), not an
    /// `if_expression`. The new rule never inspects calls.
    #[test]
    fn does_not_flag_call_with_closure_body_full_of_conditions() {
        let src = r#"
fn f(tree: &tree_sitter::Tree) {
    walk_tree(tree, |node| {
        if node.kind() != "attribute_item" { return; }
        if node.kind() == "other" || node.kind() == "third" { return; }
        do_stuff();
    });
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// Closure predicate inside an `if` condition does NOT contribute
    /// to the outer `if`'s count.
    #[test]
    fn closure_body_inside_condition_does_not_count() {
        let src = r#"
fn f(items: &[Item]) {
    if items.iter().any(|x| x.a && x.b && x.c && x.d) {
        go();
    }
}
"#;
        // Outer `if` has 0 logical ops in its own scope (the `.any(...)` is a
        // call expression; its closure body is boundary-skipped). Not flagged.
        assert!(run_on(src).is_empty());
    }
}
