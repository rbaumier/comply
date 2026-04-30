//! too-many-break-or-continue Rust backend.
//!
//! Walk loop nodes (`for_expression`, `while_expression`, `loop_expression`)
//! and count `break_expression` / `continue_expression` descendants.

use crate::diagnostic::{Diagnostic, Severity};

const LOOP_KINDS: &[&str] = &["for_expression", "while_expression", "loop_expression"];

fn count_break_continue(node: tree_sitter::Node) -> usize {
    let mut count = 0;
    walk_skip_nested_loops(node, &mut count);
    count
}

fn walk_skip_nested_loops(node: tree_sitter::Node, count: &mut usize) {
    let mut child_cursor = node.walk();
    for child in node.named_children(&mut child_cursor) {
        let kind = child.kind();
        if kind == "break_expression" || kind == "continue_expression" {
            *count += 1;
        } else if LOOP_KINDS.contains(&kind) {
            // Don't recurse into nested loops.
            continue;
        } else {
            walk_skip_nested_loops(child, count);
        }
    }
}

crate::ast_check! { on ["for_expression", "while_expression", "loop_expression"] => |node, _source, ctx, diagnostics|
    let bc_count = count_break_continue(node);
    if bc_count >= 2 {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "too-many-break-or-continue".into(),
            message: format!(
                "Loop contains {bc_count} `break`/`continue` statements \u{2014} consider refactoring."
            ),
            severity: Severity::Warning,
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
    fn flags_two_breaks() {
        let src = "fn f() { for x in arr { if a { break; } if b { break; } } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_break_and_continue() {
        let src = "fn f() { loop { if a { continue; } if b { break; } } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_single_break() {
        let src = "fn f() { for x in arr { if a { break; } do_work(); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_no_break() {
        let src = "fn f() { for x in arr { do_work(x); } }";
        assert!(run_on(src).is_empty());
    }
}
