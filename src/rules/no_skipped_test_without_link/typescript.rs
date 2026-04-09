//! no-skipped-test-without-link backend — flag `.skip` without a comment
//! referencing a tracked issue.
//!
//! Why: `.skip` disables a test. If nobody tracks why, it stays disabled
//! forever and the coverage hole becomes permanent. Require an issue link
//! in an adjacent comment so skipped tests are findable and revivable.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const TEST_BASES: &[&str] = &["test", "it", "describe", "suite", "context"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "member_expression" {
                return;
            }
            let Some(object) = node.child_by_field_name("object") else {
                return;
            };
            let Some(property) = node.child_by_field_name("property") else {
                return;
            };
            let Ok(object_text) = object.utf8_text(source_bytes) else {
                return;
            };
            let Ok(property_text) = property.utf8_text(source_bytes) else {
                return;
            };
            if !TEST_BASES.contains(&object_text) || property_text != "skip" {
                return;
            }
            if has_issue_reference_nearby(node, source_bytes) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-skipped-test-without-link".into(),
                message: format!(
                    "`{object_text}.skip` without a linked issue — add a \
                     comment referencing a ticket (`#123`, `ABC-456`, or a \
                     URL) so the skip can be revived later."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

/// Look at the previous sibling comment and check for an issue reference.
/// Reference detection is shared with `todo-needs-issue-link` via the
/// `crate::rules::issue_link` module.
fn has_issue_reference_nearby(node: tree_sitter::Node, source: &[u8]) -> bool {
    // Walk up to the nearest statement-level node and check its preceding comment.
    let mut current = node;
    while let Some(parent) = current.parent() {
        if matches!(parent.kind(), "expression_statement" | "call_expression") {
            current = parent;
        } else {
            break;
        }
    }
    let Some(prev) = current.prev_named_sibling() else {
        return false;
    };
    if prev.kind() != "comment" {
        return false;
    }
    let Ok(text) = prev.utf8_text(source) else {
        return false;
    };
    crate::rules::issue_link::has_issue_reference(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.ts"), source),
            &tree,
        )
    }

    #[test]
    fn flags_skip_without_comment() {
        assert_eq!(run_on("it.skip('x', () => {});").len(), 1);
    }

    #[test]
    fn allows_skip_with_issue_reference() {
        let source = "// Skipped — tracked in #1234\nit.skip('x', () => {});";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_skip_with_url() {
        let source =
            "// See https://github.com/foo/bar/issues/42\nit.skip('x', () => {});";
        assert!(run_on(source).is_empty());
    }
}
