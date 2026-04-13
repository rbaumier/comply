//! no-negated-condition — flag negated conditions in if/else and ternaries.
//!
//! Flags:
//! - `if (!x) { A } else { B }` — swap branches and remove `!`
//! - `if (a !== b) { A } else { B }` — use `===` and swap
//! - `!x ? A : B` — same for ternaries
//!
//! Does NOT flag if-statements without an else branch, or `else if` chains.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    match node.kind() {
        "if_statement" => {
            // Must have an `alternative` (else branch).
            let alt = match node.child_by_field_name("alternative") {
                Some(a) => a,
                None => return,
            };

            // Skip `else if` chains — the alternative is another if_statement
            // (wrapped in an else_clause whose child is if_statement).
            if alt.kind() == "else_clause" {
                let mut cursor = alt.walk();
                if cursor.goto_first_child() {
                    loop {
                        let child = cursor.node();
                        if child.kind() == "if_statement" {
                            return;
                        }
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }
            }

            let cond = match node.child_by_field_name("condition") {
                Some(c) => c,
                None => return,
            };

            if is_negated_condition(&cond, source) {
                let pos = cond.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-negated-condition".into(),
                    message: "Unexpected negated condition — swap the if/else branches \
                              and remove the negation."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        "ternary_expression" => {
            let cond = match node.child_by_field_name("condition") {
                Some(c) => c,
                None => return,
            };

            if is_negated_condition(&cond, source) {
                let pos = cond.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-negated-condition".into(),
                    message: "Unexpected negated condition — swap the ternary arms \
                              and remove the negation."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        _ => {}
    }
}

/// A condition is "negated" if it is:
/// - a `!expr` unary expression, OR
/// - a `!=` / `!==` binary expression.
///
/// The condition node from an if_statement is a `parenthesized_expression`,
/// so we unwrap it first.
fn is_negated_condition<'a>(node: &'a tree_sitter::Node<'a>, source: &[u8]) -> bool {
    let inner = unwrap_parens(node);
    match inner.kind() {
        "unary_expression" => {
            let op = inner
                .child_by_field_name("operator")
                .and_then(|o: tree_sitter::Node<'a>| o.utf8_text(source).ok())
                .unwrap_or("");
            op == "!"
        }
        "binary_expression" => {
            let op = inner
                .child_by_field_name("operator")
                .and_then(|o: tree_sitter::Node<'a>| o.utf8_text(source).ok())
                .unwrap_or("");
            op == "!=" || op == "!=="
        }
        _ => false,
    }
}

fn unwrap_parens<'a>(node: &'a tree_sitter::Node<'a>) -> tree_sitter::Node<'a> {
    let mut n = *node;
    while n.kind() == "parenthesized_expression" {
        if let Some(child) = n.named_child(0) {
            n = child;
        } else {
            break;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_negated_if_else() {
        let d = run_on("if (!x) { a(); } else { b(); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("swap the if/else"));
    }

    #[test]
    fn flags_not_equal_if_else() {
        let d = run_on("if (a !== b) { x(); } else { y(); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_loose_not_equal_if_else() {
        let d = run_on("if (a != b) { x(); } else { y(); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_if_without_else() {
        assert!(run_on("if (!x) { a(); }").is_empty());
    }

    #[test]
    fn allows_else_if() {
        assert!(run_on("if (!x) { a(); } else if (y) { b(); }").is_empty());
    }

    #[test]
    fn allows_positive_condition() {
        assert!(run_on("if (x) { a(); } else { b(); }").is_empty());
    }

    #[test]
    fn flags_negated_ternary() {
        let d = run_on("const r = !x ? a : b;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("swap the ternary"));
    }

    #[test]
    fn flags_not_equal_ternary() {
        let d = run_on("const r = a !== b ? x : y;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_positive_ternary() {
        assert!(run_on("const r = x ? a : b;").is_empty());
    }
}
