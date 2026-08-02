//! intermediate-variables Rust backend.
//!
//! Flags `if` conditions that chain three or more boolean operands via
//! `&&` / `||` and at least one operand is still an inline expression
//! — a call, a field access, a comparison. The remediation is to extract
//! those operands into named local variables so that the `if` reads as
//! one or two higher-level checks. A chain whose operands are all bare
//! identifiers carries the names already and is left alone.
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

/// Returns the single expression a `parenthesized_expression` or a
/// `unary_expression` wraps, skipping `line_comment`/`block_comment`
/// nodes that tree-sitter also reports as named children.
fn inner_operand(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    (0..node.named_child_count())
        .filter_map(|i| node.named_child(i))
        .find(|child| !matches!(child.kind(), "line_comment" | "block_comment"))
}

/// Reports whether every operand of the `&&` / `||` chain is a bare
/// identifier.
///
/// The walk descends the logical spine plus the two transparent
/// wrappers a boolean operand can carry — parentheses and a unary
/// operator (`!`, `*` and `-` are all transparent here; the TS backend
/// accepts only `!`, since `typeof`/`void` are not names). Every other
/// node kind is an operand that still holds an unnamed expression: a
/// call, a field access, a comparison.
fn every_operand_is_named(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "identifier" => true,
        "parenthesized_expression" | "unary_expression" => inner_operand(node)
            .is_some_and(|operand| every_operand_is_named(operand, source)),
        "binary_expression" => {
            let Some(op) = node.child_by_field_name("operator") else {
                return false;
            };
            let Ok(op_text) = op.utf8_text(source) else {
                return false;
            };
            let (Some(left), Some(right)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ) else {
                return false;
            };
            LOGICAL_OPS.contains(&op_text)
                && every_operand_is_named(left, source)
                && every_operand_is_named(right, source)
        }
        _ => false,
    }
}

crate::ast_check! { on ["if_expression"] => |node, source, ctx, diagnostics|
    let Some(condition) = node.child_by_field_name("condition") else { return };
    let min_ops = ctx.config.threshold("intermediate-variables", "min_ops", ctx.lang);
    if count_logical_ops(condition, source) < min_ops {
        return;
    }
    if every_operand_is_named(condition, source) {
        return;
    }
    let pos = condition.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "intermediate-variables".into(),
        message: "`if` condition chains three or more boolean operands \u{2014} extract the inline parts into named local variables.".into(),
        severity: Severity::Error,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_three_operand_and_chain() {
        let src = "fn f() { if a.is_open() && b.len() > 0 && c.ready { x(); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_four_operand_or_chain() {
        let src = "fn f() { if a.x() || b.y() || c.z() || d.w() { x(); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_mixed_and_or() {
        let src = "fn f() { if a.p() && b.q() || c.r() { x(); } }";
        // 1 && + 1 || = 2 logical ops → flag.
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_chain_of_named_operands() {
        let src = "fn f() { if a && b && c { x(); } }";
        assert!(run_on(src).is_empty());
    }

    /// tree-sitter reports comments as named children, so the walk into a
    /// parenthesized operand must step over them.
    #[test]
    fn allows_named_operands_around_a_comment() {
        let block = "fn f() { if (/* why */ a || b) && !c { x(); } }";
        assert!(run_on(block).is_empty());
        let line = "fn f() { if (\n    // why\n    a || b\n) && !c { x(); } }";
        assert!(run_on(line).is_empty());
    }

    /// Regression for #6816: clap's `parser.rs` names every operand of
    /// the chain in a preceding `let`, so the remediation is already
    /// applied and the diagnostic has nothing to ask for.
    #[test]
    fn allows_parenthesized_and_negated_named_operands() {
        let src = r#"
fn f() {
    let low_index_mults = self
        .cmd
        .get_positionals()
        .any(|a| a.get_num_args().expect("built").max_values() > 1 && a.get_index().is_some())
        && !trailing_values;

    let is_terminated = self
        .cmd
        .get_keymap()
        .get(&pos_counter)
        .map(|a| a.get_value_terminator().is_some())
        .unwrap_or_default();

    let missing_pos = self.cmd.is_allow_missing_positional_set()
        && is_second_to_last
        && !trailing_values;

    if (low_index_mults || missing_pos) && !is_terminated {
        go();
    }
}
"#;
        assert!(run_on(src).is_empty());
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
