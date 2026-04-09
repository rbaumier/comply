//! tanstack-query-array-key backend — flag `queryKey: 'some-string'`.
//!
//! Why: TanStack Query v5 requires query keys to be arrays. Strings
//! silently work in some versions and break in others. An array key is
//! also required for hierarchical invalidation: `['todos', id]` lets
//! `invalidateQueries({ queryKey: ['todos'] })` match everything.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "pair" {
                return;
            }
            let Some(key) = node.child_by_field_name("key") else {
                return;
            };
            let Ok(key_text) = key.utf8_text(source_bytes) else {
                return;
            };
            if key_text != "queryKey" && key_text != "mutationKey" {
                return;
            }
            let Some(value) = node.child_by_field_name("value") else {
                return;
            };
            if !matches!(value.kind(), "string" | "template_string") {
                return;
            }
            let pos = value.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "tanstack-query-array-key".into(),
                message: format!(
                    "`{key_text}` must be an array. Wrap the string in \
                     brackets: `['todos']` instead of `'todos'`. Array keys \
                     enable hierarchical invalidation."
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
    fn flags_string_query_key() {
        assert_eq!(
            run_on("useQuery({ queryKey: 'todos', queryFn: f });").len(),
            1
        );
    }

    #[test]
    fn allows_array_query_key() {
        assert!(
            run_on("useQuery({ queryKey: ['todos'], queryFn: f });").is_empty()
        );
    }
}
