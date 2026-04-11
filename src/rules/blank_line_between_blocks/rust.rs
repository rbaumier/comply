//! blank-line-between-blocks Rust backend.
//!
//! Missing blank lines between logical blocks (return after code,
//! function call after declaration).

use crate::diagnostic::{Diagnostic, Severity};

fn is_return_node(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "return_expression" {
        return true;
    }
    if node.kind() == "expression_statement"
        && let Some(child) = node.named_child(0)
        && child.kind() == "return_expression"
    {
        return true;
    }
    // In Rust, `return` may appear directly as text.
    let text = node.utf8_text(source).unwrap_or("");
    text.trim_start().starts_with("return ")
}

fn is_let_kind(kind: &str) -> bool {
    kind == "let_declaration"
}

fn is_comment_kind(kind: &str) -> bool {
    kind == "line_comment" || kind == "block_comment"
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "block" {
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

        // Rule: `return` preceded by a non-blank line that isn't a comment.
        if is_return_node(curr, source)
            && gap < 2
            && !is_return_node(prev, source)
            && !is_comment_kind(prev.kind())
            && !is_let_kind(prev.kind())
        {
            let pos = curr.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "blank-line-between-blocks".into(),
                message: "Add a blank line before `return`.".into(),
                severity: Severity::Warning,
            });
        }

        // Rule: call expression after let declaration without blank line.
        if is_let_kind(prev.kind())
            && !is_let_kind(curr.kind())
            && !is_comment_kind(curr.kind())
            && !is_return_node(curr, source)
            && gap < 2
        {
            let pos = curr.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "blank-line-between-blocks".into(),
                message: "Add a blank line between declarations and logic.".into(),
                severity: Severity::Warning,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_return_without_blank_line() {
        let src = "fn f() {\n    do_something();\n    return 42;\n}\n";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_return_with_blank_line() {
        let src = "fn f() {\n    do_something();\n\n    return 42;\n}\n";
        assert!(run_on(src).is_empty());
    }
}
