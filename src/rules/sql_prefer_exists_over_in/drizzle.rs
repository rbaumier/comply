//! sql-prefer-exists-over-in — Drizzle ORM backend.
//!
//! Flags `inArray(col, subquery)` calls — i.e. an `inArray` whose
//! second argument is a Drizzle query builder chain (contains both
//! `select` and `from`). `EXISTS` short-circuits on the first matching
//! row; `IN (SELECT …)` materialises the entire subquery first.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["call_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let Ok(name) = function.utf8_text(source_bytes) else {
            return;
        };
        if name.contains('.') {
            return;
        }
        if name != "inArray" {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let mut cursor = args.walk();
        let arg_nodes: Vec<tree_sitter::Node> = args
            .children(&mut cursor)
            .filter(|c| c.kind() != "(" && c.kind() != ")" && c.kind() != ",")
            .collect();
        if arg_nodes.len() < 2 {
            return;
        }
        let second = arg_nodes[1];
        let Ok(second_text) = second.utf8_text(source_bytes) else {
            return;
        };
        // A subquery contains both `.select` (or `select(`) and `.from`
        // (or `from(`). An array literal does not.
        if !looks_like_subquery(second_text) {
            return;
        }

        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "`inArray(col, subquery)` — prefer `exists()` which \
                      short-circuits on first match."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn looks_like_subquery(text: &str) -> bool {
    text.contains("select") && text.contains("from")
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_inarray_with_subquery() {
        let src = "where(inArray(users.id, db.select({ id: orders.userId }).from(orders)));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_inarray_with_array_literal() {
        let src = "where(inArray(users.role, ['admin', 'editor']));";
        assert!(run(src).is_empty());
    }
}
