//! too-many-break-or-continue Rust backend.
//!
//! Walk loop nodes (`for_expression`, `while_expression`, `loop_expression`)
//! and count flow-control `break`/`continue` descendants. A value-returning
//! `break <expr>` (a `loop` expression's return value, e.g. `break Ok(x)`) is
//! not a flow-control exit and is not counted; bare `break`, labeled
//! `break 'a`, and every `continue` are.

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
        if kind == "break_expression" {
            if is_flow_control_break(child) {
                *count += 1;
            }
        } else if kind == "continue_expression" {
            *count += 1;
        } else if LOOP_KINDS.contains(&kind) {
            // Don't recurse into nested loops.
            continue;
        } else {
            walk_skip_nested_loops(child, count);
        }
    }
}

/// A `break_expression` is flow-control unless it carries a value. In
/// tree-sitter-rust its named children are the optional `label` and the
/// optional value `_expression`; a non-`label` named child means
/// `break <expr>` (a `loop` return), which is not a flow-control exit.
fn is_flow_control_break(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    !node.named_children(&mut cursor).any(|c| c.kind() != "label")
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
            severity: Severity::Error,
            span: None,
        });
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
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

    #[test]
    fn allows_value_returning_breaks() {
        // `break Ok(x)` / `break Err(y)` are loop-return values, not
        // flow-control exits, so this `loop` expression is not flagged.
        let src = "fn f() { loop { match next() { Ok(None) => break Ok(()), Err(()) => break Err(()), _ => {} } } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_mixed_value_and_bare_breaks() {
        // Two bare flow-control breaks still reach the threshold; the
        // value-returning break is not counted.
        let src = "fn f() { loop { if a { break; } if b { break; } if c { break Ok(()); } } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn value_break_keeps_count_below_threshold() {
        // One bare break + one value-returning break: only the bare one counts,
        // so the count is 1 and the loop is not flagged.
        let src = "fn f() { loop { if a { break; } if c { break Ok(()); } } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_labeled_bare_breaks() {
        // `break 'outer` carries no value: still a flow-control exit, counted.
        let src = "fn f() { 'outer: loop { if a { break 'outer; } if b { break 'outer; } } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_labeled_value_returning_breaks() {
        // `break 'outer Ok(x)` carries both a label and a value: still a
        // loop-return, not counted.
        let src = "fn f() { 'outer: loop { if a { break 'outer Ok(()); } if b { break 'outer Err(()); } } }";
        assert!(run_on(src).is_empty());
    }
}
