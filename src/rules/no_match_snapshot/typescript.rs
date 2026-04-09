//! no-match-snapshot backend — flag `toMatchSnapshot()` / `toMatchInlineSnapshot()`.
//!
//! Why: snapshot tests are a maintenance trap. They capture the output
//! shape at one moment, then every unrelated refactor breaks them and
//! developers blindly update the snapshot. The test no longer asserts
//! anything specific — it asserts "whatever the code currently produces".
//! Assert on specific fields instead.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "call_expression" {
                return;
            }
            let Some(function) = node.child_by_field_name("function") else {
                return;
            };
            if function.kind() != "member_expression" {
                return;
            }
            let Some(property) = function.child_by_field_name("property") else {
                return;
            };
            let Ok(method) = property.utf8_text(source_bytes) else {
                return;
            };
            if method != "toMatchSnapshot" && method != "toMatchInlineSnapshot" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-match-snapshot".into(),
                message: format!(
                    "`{method}()` is a maintenance trap — unrelated \
                     refactors break it and reviewers blindly update \
                     snapshots. Assert on specific fields instead."
                ),
                severity: Severity::Warning,
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
    fn flags_to_match_snapshot() {
        assert_eq!(run_on("expect(x).toMatchSnapshot();").len(), 1);
    }

    #[test]
    fn flags_to_match_inline_snapshot() {
        assert_eq!(run_on("expect(x).toMatchInlineSnapshot('y');").len(), 1);
    }

    #[test]
    fn allows_specific_assertions() {
        assert!(run_on("expect(x.foo).toBe('bar');").is_empty());
    }
}
