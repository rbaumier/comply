//! too-many-break-or-continue backend — walk loop nodes (`for_statement`,
//! `for_in_statement`, `while_statement`, `do_statement`) and count
//! `break_statement` / `continue_statement` descendants.
//!
//! Detection: for each loop, count direct break/continue descendants
//! (excluding those inside nested loops). Flag if >= 2.

use crate::diagnostic::{Diagnostic, Severity};

const LOOP_KINDS: &[&str] = &[
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
];

/// Count `break_statement` and `continue_statement` nodes that are direct
/// children of this loop (not nested inside inner loops).
fn count_break_continue(node: tree_sitter::Node) -> usize {
    let mut count = 0;
    let mut cursor = node.walk();
    walk_skip_nested_loops(node, &mut cursor, &mut count);
    count
}

fn walk_skip_nested_loops(
    node: tree_sitter::Node,
    _cursor: &mut tree_sitter::TreeCursor,
    count: &mut usize,
) {
    let mut child_cursor = node.walk();
    for child in node.named_children(&mut child_cursor) {
        let kind = child.kind();
        if kind == "break_statement" || kind == "continue_statement" {
            *count += 1;
        } else if LOOP_KINDS.contains(&kind) {
            // Don't recurse into nested loops
            continue;
        } else {
            walk_skip_nested_loops(child, _cursor, count);
        }
    }
}

crate::ast_check! { |node, _source, ctx, diagnostics|
    if !LOOP_KINDS.contains(&node.kind()) {
        return;
    }
    let bc_count = count_break_continue(node);
    if bc_count >= 2 {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "too-many-break-or-continue".into(),
            message: format!(
                "Loop contains {bc_count} `break`/`continue` statements — consider refactoring."
            ),
            severity: Severity::Warning,
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_two_breaks() {
        let src = "for (const x of arr) {\n  if (a) break;\n  if (b) break;\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_break_and_continue() {
        let src = "while (true) {\n  if (a) continue;\n  if (b) break;\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_single_break() {
        let src = "for (const x of arr) {\n  if (a) break;\n  doWork();\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_no_break() {
        let src = "for (const x of arr) {\n  doWork(x);\n}";
        assert!(run_on(src).is_empty());
    }
}
