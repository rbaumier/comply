//! no-nested-assignment backend — flag assignments inside conditions.
//!
//! Detects `if (x = …)`, `while (x = …)` patterns where an assignment
//! operator appears inside a condition expression.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_statement", "while_statement"] => |node, source, _ctx, diagnostics|
match node.kind() {
        "if_statement" | "while_statement" => {}
        _ => return,
    }
    let Some(condition) = node.child_by_field_name("condition") else {
        return;
    };
    if contains_assignment(condition) {
        let pos = condition.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&_ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-nested-assignment".into(),
            message: "Assignment inside a condition — likely a bug, use `===` for comparison or move the assignment out.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Recursively check if a node or any of its children is an assignment_expression.
fn contains_assignment(node: tree_sitter::Node) -> bool {
    if node.kind() == "assignment_expression" {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if contains_assignment(child) {
            return true;
        }
    }
    false
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
    fn flags_assignment_in_if() {
        assert_eq!(run_on("if (x = 10) {}").len(), 1);
    }

    #[test]
    fn flags_assignment_in_while() {
        assert_eq!(run_on("while (node = node.next) {}").len(), 1);
    }

    #[test]
    fn allows_equality_check() {
        assert!(run_on("if (x === 10) {}").is_empty());
    }

    #[test]
    fn allows_loose_equality() {
        assert!(run_on("if (x == 10) {}").is_empty());
    }

    #[test]
    fn allows_comparison_operators() {
        assert!(run_on("if (x <= 10) {}").is_empty());
        assert!(run_on("if (x >= 10) {}").is_empty());
    }
}
