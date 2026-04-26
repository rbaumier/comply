//! no-useless-increment AST backend — `return x++` returns the value
//! before the mutation, which is almost always a bug.
//!
//! Walks `return_statement` nodes whose returned expression is a postfix
//! `update_expression` (`x++` or `x--`). Prefix updates (`++x`) are fine
//! because they evaluate to the new value.

use crate::diagnostic::{Diagnostic, Severity};

/// True for `x++` / `x--` (postfix), false for `++x` / `--x` (prefix).
fn is_postfix_update(update: tree_sitter::Node) -> bool {
    let Some(arg) = update.child_by_field_name("argument") else { return false };
    let Some(op) = update.child_by_field_name("operator") else { return false };
    arg.start_byte() < op.start_byte()
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = source;
    if node.kind() != "return_statement" {
        return;
    }

    // The returned expression is the first named child, if any.
    let Some(value) = node.named_child(0) else { return };
    if value.kind() != "update_expression" {
        return;
    }
    if !is_postfix_update(value) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-useless-increment",
        "`return x++` / `return x--` returns the value before the mutation — use prefix or separate statements.".into(),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_return_post_increment() {
        assert_eq!(run_on("return x++;").len(), 1);
    }

    #[test]
    fn flags_return_post_decrement() {
        assert_eq!(run_on("return count--;").len(), 1);
    }

    #[test]
    fn allows_prefix_increment() {
        assert!(run_on("return ++x;").is_empty());
    }

    #[test]
    fn allows_plain_return() {
        assert!(run_on("return x;").is_empty());
    }
}
