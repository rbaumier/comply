//! no-negated-condition Rust backend — flag `if !x { A } else { B }`.
//!
//! Flags if_expression with a negated condition (`!x` or `!=`) that has
//! an else clause (but not `else if`).

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
            path: ctx.path.to_path_buf(),
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
            op == "!="
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
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
}
