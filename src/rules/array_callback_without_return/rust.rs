//! array-callback-without-return Rust backend.
//!
//! Flag iterator method closures with block body but no return/expression.
//! In Rust: `.map(|x| { ... })` with block body missing a trailing expression.

use crate::diagnostic::{Diagnostic, Severity};

const ITERATOR_METHODS: &[&str] = &[
    "map", "filter", "find", "any", "all", "flat_map", "filter_map",
];

fn is_iterator_method_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else { return false };
    if func.kind() != "field_expression" {
        return false;
    }
    let Some(field) = func.child_by_field_name("field") else { return false };
    let name = field.utf8_text(source).unwrap_or("");
    ITERATOR_METHODS.contains(&name)
}

fn has_return_or_tail_expr(node: tree_sitter::Node) -> bool {
    if node.kind() == "return_expression" {
        return true;
    }
    if matches!(node.kind(), "closure_expression" | "function_item") {
        return false;
    }
    // In a block, check the last child (tail expression).
    if node.kind() == "block"
        && let Some(last) = node.named_child(node.named_child_count().saturating_sub(1))
        // A non-statement expression at end is an implicit return.
        && last.kind() != "let_declaration"
        && last.kind() != "expression_statement"
        && last.kind() != "empty_statement"
    {
        return true;
    }
    let count = node.child_count();
    for i in 0..count {
        if has_return_or_tail_expr(node.child(i).unwrap()) {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_iterator_method_call(node, source) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(callback) = args.named_child(0) else { return };

    if callback.kind() != "closure_expression" {
        return;
    }
    let Some(body) = callback.child_by_field_name("body") else { return };
    if body.kind() != "block" {
        return;
    }

    if !has_return_or_tail_expr(body) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "array-callback-without-return".into(),
            message: "Iterator callback with block body but no return value.".into(),
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
    fn flags_map_without_return() {
        let src = "fn f() { vec![1].iter().map(|x| { let y = x; }); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_map_with_tail_expr() {
        let src = "fn f() { vec![1].iter().map(|x| { x + 1 }); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_concise_closure() {
        let src = "fn f() { vec![1].iter().map(|x| x + 1); }";
        assert!(run_on(src).is_empty());
    }
}
