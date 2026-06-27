//! no-negated-condition Rust backend — flag `if !x { A } else { B }`.
//!
//! Flags if_expression with a negated condition (`!x` or `!=`) that has
//! an else clause (but not `else if`). The bitmask membership idiom
//! `(expr & mask) != 0` is exempt: it is a positive "is this bit set?"
//! assertion, not an invertible negation.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_expression"] => |node, source, ctx, diagnostics|
    // Must have an else clause.
    let Some(alt) = node.child_by_field_name("alternative") else { return };

    // Skip `else if` chains.
    if alt.kind() == "else_clause" {
        let mut cursor = alt.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "if_expression" {
                    return;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    let Some(cond) = node.child_by_field_name("condition") else { return };

    if is_negated_condition(&cond, source) {
        let pos = cond.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-negated-condition".into(),
            message: "Unexpected negated condition \u{2014} swap the if/else branches \
                      and remove the negation."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_negated_condition(node: &tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "unary_expression" => {
            // In tree-sitter-rust, unary_expression has no fields:
            // child(0) is the operator.
            let op = node
                .child(0)
                .and_then(|o| o.utf8_text(source).ok())
                .unwrap_or("");
            op == "!"
        }
        "binary_expression" => {
            let op = node
                .child_by_field_name("operator")
                .and_then(|o| o.utf8_text(source).ok())
                .unwrap_or("");
            // `(expr & mask) != 0` is the idiomatic positive "is this bit set?"
            // membership test, not an invertible negation.
            op == "!=" && !is_bitmask_zero_test(node, source)
        }
        _ => false,
    }
}

/// True for the bitmask membership test `(expr & mask) != 0` (or the mirrored
/// `0 != (expr & mask)`): one operand is a bitwise-AND expression and the other
/// is an integer literal whose value is zero.
fn is_bitmask_zero_test(node: &tree_sitter::Node, source: &[u8]) -> bool {
    let Some(left) = node.child_by_field_name("left") else {
        return false;
    };
    let Some(right) = node.child_by_field_name("right") else {
        return false;
    };
    (is_bitwise_and(&left, source) && is_zero_literal(&right, source))
        || (is_bitwise_and(&right, source) && is_zero_literal(&left, source))
}

/// True if `node` (after unwrapping parentheses) is a `<expr> & <expr>` bitwise-AND.
fn is_bitwise_and(node: &tree_sitter::Node, source: &[u8]) -> bool {
    let inner = unwrap_parens(node);
    inner.kind() == "binary_expression"
        && inner
            .child_by_field_name("operator")
            .and_then(|o| o.utf8_text(source).ok())
            == Some("&")
}

/// True if `node` is an integer literal whose value is zero (any radix, with
/// optional `_` separators and integer type suffix: `0`, `0u32`, `0x0`, ...).
fn is_zero_literal(node: &tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "integer_literal" {
        return false;
    }
    let Ok(text) = node.utf8_text(source) else {
        return false;
    };
    let cleaned: String = text.chars().filter(|&c| c != '_').collect();
    let body = cleaned
        .strip_prefix("0x")
        .or_else(|| cleaned.strip_prefix("0o"))
        .or_else(|| cleaned.strip_prefix("0b"))
        .unwrap_or(&cleaned);
    // A type suffix starts at the first `u`/`i`, neither of which is a radix digit.
    let digits = match body.find(['u', 'i']) {
        Some(idx) => &body[..idx],
        None => body,
    };
    !digits.is_empty() && digits.bytes().all(|b| b == b'0')
}

/// Strip enclosing `parenthesized_expression` layers to reach the inner expression.
fn unwrap_parens<'a>(node: &tree_sitter::Node<'a>) -> tree_sitter::Node<'a> {
    let mut current = *node;
    while current.kind() == "parenthesized_expression" {
        match current.named_child(0) {
            Some(inner) => current = inner,
            None => break,
        }
    }
    current
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
    fn flags_negated_if_else() {
        let d = run_on("fn f(x: bool) { if !x { a(); } else { b(); } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("swap the if/else"));
    }

    #[test]
    fn flags_not_equal_if_else() {
        let d = run_on("fn f(a: i32, b: i32) { if a != b { x(); } else { y(); } }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_if_without_else() {
        assert!(run_on("fn f(x: bool) { if !x { a(); } }").is_empty());
    }

    #[test]
    fn allows_else_if() {
        assert!(run_on("fn f(x: bool, y: bool) { if !x { a(); } else if y { b(); } }").is_empty());
    }

    #[test]
    fn allows_positive_condition() {
        assert!(run_on("fn f(x: bool) { if x { a(); } else { b(); } }").is_empty());
    }

    #[test]
    fn allows_bitmask_test() {
        // `(expr & mask) != 0` is the positive "is this bit set?" idiom.
        assert!(run_on(
            "fn f(x: u32) { if x & 0x80_00_00 != 0 { a(); } else { b(); } }"
        )
        .is_empty());
    }

    #[test]
    fn allows_parenthesized_bitmask_test() {
        assert!(run_on("fn f(x: u32) { if (x & 0x80) != 0 { a(); } else { b(); } }").is_empty());
    }

    #[test]
    fn allows_mirrored_bitmask_test() {
        assert!(run_on("fn f(x: u32) { if 0 != x & 0x80 { a(); } else { b(); } }").is_empty());
    }

    #[test]
    fn flags_non_bitmask_nonzero_test() {
        // Only the bitwise-AND-vs-zero shape is exempt; a bare `x != 0` is not.
        let d = run_on("fn f(x: i32) { if x != 0 { a(); } else { b(); } }");
        assert_eq!(d.len(), 1);
    }
}
