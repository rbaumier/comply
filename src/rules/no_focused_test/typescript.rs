//! no-focused-test backend — flag `.only` on test/it/describe.
//!
//! Why: a single `it.only` committed to main silently disables every
//! other test in the suite. CI runs, reports green, and regressions slip
//! through because only the one focused test actually ran. The cost of
//! committing a focused test is catastrophically asymmetric — catch it
//! at the linter.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const TEST_BASES: &[&str] = &["test", "it", "describe", "suite", "context"];

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
            if !TEST_BASES.contains(&object_text) || property_text != "only" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-focused-test".into(),
                message: format!(
                    "`{object_text}.only` silently disables every other \
                     test in the suite when committed. Remove `.only` \
                     before pushing."
                ),
                severity: Severity::Error,
            });
        });
        diagnostics
    }
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
    fn flags_test_only() {
        assert_eq!(run_on("test.only('x', () => {});").len(), 1);
    }

    #[test]
    fn flags_it_only() {
        assert_eq!(run_on("it.only('x', () => {});").len(), 1);
    }

    #[test]
    fn flags_describe_only() {
        assert_eq!(run_on("describe.only('x', () => {});").len(), 1);
    }

    #[test]
    fn allows_regular_test() {
        assert!(run_on("test('x', () => {});").is_empty());
    }
}
