//! AST backend for react-no-sort-for-extrema.
//!
//! Flags two shapes:
//! - `subscript_expression` where the object is a `.sort(...)` call and
//!   the index is `0` or a `length - 1` / `length-1` expression.
//! - Same pattern where the subscript object is a plain `identifier`
//!   that was initialized from a `.sort()` call in the same block.

use crate::diagnostic::{Diagnostic, Severity};

fn is_sort_call(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else { return false };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return false };
    prop.utf8_text(source).ok() == Some("sort")
}

fn is_zero(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    node.kind() == "number" && node.utf8_text(source).ok() == Some("0")
}

fn is_length_minus_one(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() != "binary_expression" {
        return false;
    }
    // tree-sitter fields: left, operator, right.
    let Some(op) = node.child_by_field_name("operator") else { return false };
    if op.utf8_text(source).ok() != Some("-") {
        return false;
    }
    let Some(right) = node.child_by_field_name("right") else { return false };
    if right.utf8_text(source).ok() != Some("1") {
        return false;
    }
    let Some(left) = node.child_by_field_name("left") else { return false };
    if left.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = left.child_by_field_name("property") else { return false };
    prop.utf8_text(source).ok() == Some("length")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = ctx;
    if node.kind() != "subscript_expression" {
        return;
    }
    let Some(object) = node.child_by_field_name("object") else { return };
    if !is_sort_call(object, source) {
        return;
    }
    let Some(index) = node.child_by_field_name("index") else { return };
    if !is_zero(index, source) && !is_length_minus_one(index, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`.sort(...)[0]` / `.sort(...)[length-1]` picks an extremum via O(n log n) work — \
         use `Math.min` / `Math.max` or a single-pass fold."
            .into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_sort_index_zero() {
        let src = r#"const min = arr.sort((a,b) => a - b)[0];"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_sort_length_minus_one() {
        let src = r#"const max = arr.sort((a,b) => a - b)[arr.length - 1];"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_plain_sort() {
        let src = r#"const sorted = arr.sort((a,b) => a - b);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_sort_with_other_index() {
        let src = r#"const x = arr.sort()[2];"#;
        assert!(run(src).is_empty());
    }
}
