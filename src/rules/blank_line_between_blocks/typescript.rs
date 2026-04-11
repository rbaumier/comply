//! blank-line-between-blocks AST backend — missing blank lines between
//! logical blocks (return after code, function call after declaration).
//!
//! Even as an AST check, this rule uses line positions (`start_position().row`)
//! to detect missing blank lines between adjacent statements.

use crate::diagnostic::{Diagnostic, Severity};

fn is_return_kind(kind: &str) -> bool {
    kind == "return_statement"
}

fn is_declaration_kind(kind: &str) -> bool {
    kind == "lexical_declaration" || kind == "variable_declaration"
}

fn is_expression_statement_call(node: tree_sitter::Node) -> bool {
    if node.kind() != "expression_statement" {
        return false;
    }
    if let Some(child) = node.named_child(0) {
        return child.kind() == "call_expression" || child.kind() == "await_expression";
    }
    false
}

fn is_comment_kind(kind: &str) -> bool {
    kind == "comment"
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // We look at statement_block nodes and check sequential children.
    if node.kind() != "statement_block" {
        return;
    }

    let child_count = node.named_child_count();
    if child_count < 2 {
        return;
    }

    for i in 1..child_count {
        let prev = node.named_child(i - 1).unwrap();
        let curr = node.named_child(i).unwrap();

        let prev_end_row = prev.end_position().row;
        let curr_start_row = curr.start_position().row;
        let gap = curr_start_row.saturating_sub(prev_end_row);

        // Rule 1: `return` preceded by a non-blank line that isn't `}` or a comment.
        if is_return_kind(curr.kind())
            && gap < 2
            && !is_return_kind(prev.kind())
            && !is_comment_kind(prev.kind())
            && prev.kind() != "}"
        {
            let pos = curr.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "blank-line-between-blocks".into(),
                message: "Add a blank line before `return` for visual separation.".into(),
                severity: Severity::Warning,
            });
        }

        // Rule 2: function call immediately after a declaration without blank line.
        if is_expression_statement_call(curr) && is_declaration_kind(prev.kind()) && gap < 2 {
            let pos = curr.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "blank-line-between-blocks".into(),
                message: "Add a blank line between declarations and function calls.".into(),
                severity: Severity::Warning,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_return_without_blank_line() {
        let src = "function f() {\n  const x = 1;\n  return x;\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_return_after_blank_line() {
        let src = "function f() {\n  const x = 1;\n\n  return x;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_declaration_then_call() {
        let src = "function f() {\n  const x = getX();\n  doSomething(x);\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_declaration_then_call_with_blank() {
        let src = "function f() {\n  const x = getX();\n\n  doSomething(x);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_consecutive_declarations() {
        let src = "function f() {\n  const a = 1;\n  const b = 2;\n}";
        assert!(run_on(src).is_empty());
    }
}
