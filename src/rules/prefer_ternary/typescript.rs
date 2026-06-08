//! prefer-ternary — flag simple if/else assignments that can be ternaries.
//!
//! Matches `if (cond) { x = a; } else { x = b; }` where both branches
//! assign to the same target, and suggests `x = cond ? a : b;`.
//! Also handles `return` in both branches.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_statement"] => |node, source, ctx, diagnostics|
    // Skip if this is an `else if` (this if_statement is the alternate of a parent if).
    if let Some(parent) = node.parent()
        && parent.kind() == "else_clause" {
            return;
        }

    let consequence = match node.child_by_field_name("consequence") {
        Some(c) => c,
        None => return,
    };
    let alternative = match node.child_by_field_name("alternative") {
        Some(a) => a,
        None => return,
    };

    let cons_body = unwrap_block(&consequence);
    let alt_body = unwrap_else_clause(&alternative);

    let cons_body = match cons_body {
        Some(b) => b,
        None => return,
    };
    let alt_body = match alt_body {
        Some(b) => b,
        None => return,
    };

    // Both must be single expression_statements or return_statements.
    let cons_inner = single_statement_body(&cons_body, source);
    let alt_inner = single_statement_body(&alt_body, source);

    let (cons_inner, alt_inner) = match (cons_inner, alt_inner) {
        (Some(c), Some(a)) => (c, a),
        _ => return,
    };

    // Case 1: both are assignments to the same target.
    // tree-sitter uses `assignment_expression` for `=` and
    // `augmented_assignment_expression` for `+=`, `-=`, etc.
    if cons_inner.kind() == "expression_statement" && alt_inner.kind() == "expression_statement" {
        let cons_expr = match cons_inner.named_child(0) {
            Some(e) => e,
            None => return,
        };
        let alt_expr = match alt_inner.named_child(0) {
            Some(e) => e,
            None => return,
        };

        let is_plain_assign =
            cons_expr.kind() == "assignment_expression" && alt_expr.kind() == "assignment_expression";
        let is_augmented = cons_expr.kind() == "augmented_assignment_expression"
            && alt_expr.kind() == "augmented_assignment_expression";

        if !is_plain_assign && !is_augmented {
            return;
        }

        // For augmented assignments, the operators must match.
        if is_augmented {
            let cons_op = cons_expr
                .child_by_field_name("operator")
                .and_then(|o| o.utf8_text(source).ok())
                .unwrap_or("");
            let alt_op = alt_expr
                .child_by_field_name("operator")
                .and_then(|o| o.utf8_text(source).ok())
                .unwrap_or("");
            if cons_op != alt_op {
                return;
            }
        }

        // Same left-hand side
        let cons_lhs = cons_expr
            .child_by_field_name("left")
            .and_then(|l| l.utf8_text(source).ok())
            .unwrap_or("");
        let alt_lhs = alt_expr
            .child_by_field_name("left")
            .and_then(|l| l.utf8_text(source).ok())
            .unwrap_or("");
        if cons_lhs.trim() != alt_lhs.trim() || cons_lhs.trim().is_empty() {
            return;
        }

        let op_display = if is_augmented {
            cons_expr
                .child_by_field_name("operator")
                .and_then(|o| o.utf8_text(source).ok())
                .unwrap_or("=")
        } else {
            "="
        };

        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "prefer-ternary".into(),
            message: format!(
                "This `if` statement can be replaced by a ternary: \
                 `{lhs} {op} cond ? consequent : alternate`.",
                lhs = cons_lhs.trim(),
                op = op_display,
            ),
            severity: Severity::Warning,
            span: None,
        });
        return;
    }

    // Case 2: both are return statements.
    if cons_inner.kind() == "return_statement" && alt_inner.kind() == "return_statement" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "prefer-ternary".into(),
            message: "This `if` statement can be replaced by `return cond ? a : b;`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// If node is a `statement_block`, return it; otherwise return the node itself
/// if it's a single statement.
fn unwrap_block<'a>(node: &'a tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    Some(*node)
}

/// Unwrap an else_clause to get its body.
fn unwrap_else_clause<'a>(node: &'a tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    if node.kind() == "else_clause" {
        // The else_clause's child is either a statement_block or an if_statement.
        for i in 0..node.named_child_count() {
            if let Some(child) = node.named_child(i) {
                // Skip `else if` — don't suggest ternary for chains.
                if child.kind() == "if_statement" {
                    return None;
                }
                return Some(child);
            }
        }
        None
    } else {
        Some(*node)
    }
}

/// Extract the single meaningful statement from a block or bare statement.
fn single_statement_body<'a>(
    node: &tree_sitter::Node<'a>,
    _source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    if node.kind() == "statement_block" {
        let mut stmts = Vec::new();
        for i in 0..node.named_child_count() {
            if let Some(child) = node.named_child(i) {
                if child.kind() == "empty_statement" {
                    continue;
                }
                stmts.push(child);
            }
        }
        if stmts.len() == 1 {
            Some(stmts[0])
        } else {
            None
        }
    } else if node.kind() == "expression_statement" || node.kind() == "return_statement" {
        Some(*node)
    } else {
        None
    }
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_simple_assignment_if_else() {
        let d = run_on("if (cond) { x = a; } else { x = b; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("ternary"));
    }

    #[test]
    fn flags_return_if_else() {
        let d = run_on("function f() { if (cond) { return a; } else { return b; } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("return"));
    }

    #[test]
    fn allows_different_targets() {
        assert!(run_on("if (c) { x = 1; } else { y = 2; }").is_empty());
    }

    #[test]
    fn allows_multi_statement_branches() {
        assert!(run_on("if (c) { x = 1; log(); } else { x = 2; }").is_empty());
    }

    #[test]
    fn allows_if_without_else() {
        assert!(run_on("if (c) { x = 1; }").is_empty());
    }

    #[test]
    fn allows_else_if_chain() {
        assert!(run_on("if (a) { x = 1; } else if (b) { x = 2; } else { x = 3; }").is_empty());
    }

    #[test]
    fn flags_compound_assignment() {
        let d = run_on("if (cond) { x += a; } else { x += b; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn rejects_different_operators() {
        // `=` vs `+=` are different node kinds, so they don't match.
        assert!(run_on("if (c) { x = 1; } else { x += 2; }").is_empty());
    }

    #[test]
    fn rejects_different_augmented_operators() {
        // `+=` vs `-=` are both augmented but different operators.
        assert!(run_on("if (c) { x += 1; } else { x -= 2; }").is_empty());
    }
}
