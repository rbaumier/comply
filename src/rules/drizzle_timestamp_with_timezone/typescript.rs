//! drizzle-timestamp-with-timezone backend — flag `timestamp('col')`
//! without `{ withTimezone: true }`.
//!
//! Why: bare `timestamp` columns are ambiguous across time zones. When
//! servers, clients, and databases live in different zones, `'2024-01-01
//! 12:00'` can mean three different points in time. `withTimezone: true`
//! stores an absolute instant and eliminates the ambiguity.

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
            if node.kind() != "call_expression" {
                return;
            }
            let Some(function) = node.child_by_field_name("function") else {
                return;
            };
            let Ok(name) = function.utf8_text(source_bytes) else {
                return;
            };
            if name != "timestamp" {
                return;
            }
            let Some(args) = node.child_by_field_name("arguments") else {
                return;
            };
            // Expect 1 arg (column name only) — 2 args means options were passed.
            if args.named_child_count() >= 2 {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "drizzle-timestamp-with-timezone".into(),
                message: "`timestamp('col')` without `{ withTimezone: true }` \
                          — ambiguous across time zones. Always use \
                          `timestamp('col', { withTimezone: true })`."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_bare_timestamp() {
        assert_eq!(run_on("const t = timestamp('created_at');").len(), 1);
    }

    #[test]
    fn allows_timestamp_with_options() {
        assert!(
            run_on("const t = timestamp('created_at', { withTimezone: true });").is_empty()
        );
    }
}
