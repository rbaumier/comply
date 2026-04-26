//! prefer-single-boolean-return AST backend.
//!
//! Three AST shapes trigger:
//!   1) `if (cond) { return <bool>; } else { return <bool>; }` where the
//!      booleans are opposite.
//!   2) `if (cond) return <bool>; else return <bool>;` (no braces).
//!   3) Sibling form: `if (cond) { return <bool>; } return <bool>;`
//!      inside a surrounding block — the `else` is implicit.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_statement", "statement_block"] => |node, source, ctx, diagnostics|
match node.kind() {
        "if_statement" => check_if_else(node, source, ctx, diagnostics),
        "statement_block" => check_sibling_return(node, source, ctx, diagnostics),
        _ => {}
    }
}

fn check_if_else(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(cons) = node.child_by_field_name("consequence") else { return };
    let Some(alt) = node.child_by_field_name("alternative") else { return };

    // `alternative` is an `else_clause` wrapping the real body.
    let alt_body = unwrap_else(alt);
    // Skip `else if` — the alternative is itself an if_statement.
    if alt_body.kind() == "if_statement" {
        return;
    }

    let Some(cons_bool) = extract_single_return_bool(cons, source) else { return };
    let Some(alt_bool) = extract_single_return_bool(alt_body, source) else { return };
    if cons_bool == alt_bool {
        return;
    }

    push_diag(node, ctx, diagnostics);
}

/// Detect `if (...) return <bool>;` followed by `return <bool>;` as
/// sibling children of the same block.
fn check_sibling_return(
    block: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut cursor = block.walk();
    let children: Vec<_> = block.named_children(&mut cursor).collect();
    for i in 0..children.len().saturating_sub(1) {
        let first = children[i];
        let second = children[i + 1];
        if first.kind() != "if_statement" {
            continue;
        }
        // Must have no `else` branch.
        if first.child_by_field_name("alternative").is_some() {
            continue;
        }
        let Some(cons) = first.child_by_field_name("consequence") else { continue };
        let Some(first_bool) = extract_single_return_bool(cons, source) else { continue };
        if second.kind() != "return_statement" {
            continue;
        }
        let Some(second_bool) = return_bool_value(second, source) else { continue };
        if first_bool == second_bool {
            continue;
        }
        push_diag(first, ctx, diagnostics);
    }
}

fn push_diag(
    node: tree_sitter::Node,
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-single-boolean-return".into(),
        message: "`if (cond) return <bool>; else return <bool>;` — return the condition directly.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

fn unwrap_else(alt: tree_sitter::Node) -> tree_sitter::Node {
    if alt.kind() != "else_clause" {
        return alt;
    }
    let mut cursor = alt.walk();
    alt.named_children(&mut cursor).next().unwrap_or(alt)
}

/// If `node` is a return_statement returning a boolean literal, return it.
/// If `node` is a statement_block with a single `return <bool>;`, return the bool.
fn extract_single_return_bool(node: tree_sitter::Node, source: &[u8]) -> Option<bool> {
    if node.kind() == "return_statement" {
        return return_bool_value(node, source);
    }
    if node.kind() == "statement_block" {
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        if children.len() != 1 {
            return None;
        }
        if children[0].kind() == "return_statement" {
            return return_bool_value(children[0], source);
        }
    }
    None
}

fn return_bool_value(ret: tree_sitter::Node, source: &[u8]) -> Option<bool> {
    let mut cursor = ret.walk();
    let value = ret.named_children(&mut cursor).next()?;
    match value.utf8_text(source).ok()? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_if_else_block_true_false() {
        let src = r#"function f(x: boolean) { if (x) { return true; } else { return false; } }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_if_else_block_false_true() {
        let src = r#"function f(x: boolean) { if (x) { return false; } else { return true; } }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_if_else_no_braces() {
        let src = r#"function f(x: boolean) { if (x) return true; else return false; }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_sibling_return_form() {
        let src = r#"function f(x: boolean) {
    if (x) { return true; }
    return false;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_sibling_no_braces() {
        let src = r#"function f(x: boolean) {
    if (x) return true;
    return false;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_different_return_values() {
        let src = r#"function f(x: boolean) { if (x) { return 1; } else { return 2; } }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_same_bool_both_branches() {
        let src = r#"function f(x: boolean) { if (x) { return true; } else { return true; } }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn skips_outer_else_if_chain() {
        // The outer `if` has an `else if` tail so it is NOT flagged.
        // The inner `if (x === 2) return false; else return true;` IS a
        // simplifiable shape — one diagnostic is expected.
        let src = r#"function f(x: number) {
    if (x === 1) return true;
    else if (x === 2) return false;
    else return true;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_extra_statement_in_branch() {
        let src = r#"function f(x: boolean) {
    if (x) {
        log();
        return true;
    } else {
        return false;
    }
}"#;
        assert!(run_on(src).is_empty());
    }
}
