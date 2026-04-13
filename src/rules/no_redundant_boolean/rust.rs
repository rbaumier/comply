//! no-redundant-boolean Rust backend.
//!
//! Detect `if x { true } else { false }` -> just `x`,
//! and `== true` / `== false` comparisons.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        walk_tree(tree, |node| {
            match node.kind() {
                "if_expression" => {
                    check_if_returning_bool(node, source, ctx, &mut diagnostics);
                }
                "binary_expression" => {
                    check_bool_comparison(node, source, ctx, &mut diagnostics);
                }
                _ => {}
            }
        });

        diagnostics
    }
}

fn check_if_returning_bool(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(consequence) = node.child_by_field_name("consequence") else { return };
    let Some(alternative) = node.child_by_field_name("alternative") else { return };

    let then_text = block_single_expr_text(consequence, source);
    let else_text = else_single_expr_text(alternative, source);

    let Some(then_val) = then_text else { return };
    let Some(else_val) = else_text else { return };

    if (then_val == "true" && else_val == "false")
        || (then_val == "false" && else_val == "true")
    {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-redundant-boolean".into(),
            message: "Redundant if/else returning boolean literals \u{2014} return the condition directly.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn block_single_expr_text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "block" {
        return None;
    }
    if node.named_child_count() != 1 {
        return None;
    }
    let child = node.named_child(0)?;
    let text = child.utf8_text(source).ok()?.trim();
    // Handle both `return true;` and just `true`
    let text = text.strip_prefix("return ").unwrap_or(text);
    let text = text.strip_suffix(';').unwrap_or(text).trim();
    Some(text)
}

fn else_single_expr_text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    // else_clause wraps a block
    if node.kind() == "else_clause" {
        let block = node.named_child(0)?;
        return block_single_expr_text(block, source);
    }
    block_single_expr_text(node, source)
}

fn check_bool_comparison(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let Ok(op) = op_node.utf8_text(source) else { return };

    if op != "==" && op != "!=" {
        return;
    }

    let Some(right) = node.child_by_field_name("right") else { return };
    let Ok(right_text) = right.utf8_text(source) else { return };

    if right_text.trim() == "true" || right_text.trim() == "false" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-redundant-boolean".into(),
            message: "Redundant boolean comparison \u{2014} use the value directly.".into(),
            severity: Severity::Error,
            span: None,
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
    fn flags_if_true_else_false() {
        let src = r#"fn f(x: bool) -> bool { if x { true } else { false } }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_eq_true() {
        let src = "fn f(x: bool) { if x == true {} }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_normal_if() {
        let src = r#"fn f(x: bool) -> &str { if x { "a" } else { "b" } }"#;
        assert!(run_on(src).is_empty());
    }
}
