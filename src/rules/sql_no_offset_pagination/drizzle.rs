//! sql-no-offset-pagination — Drizzle ORM backend.
//!
//! Flags `.offset(n)` calls on a Drizzle query builder chain. To confirm the
//! call belongs to a query (and not just any object with an `offset` method),
//! we walk the receiver subtree looking for a `.from(...)` or `.select(...)`
//! call.

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
        if function.kind() != "member_expression" {
            return;
        }
        let Some(prop) = function.child_by_field_name("property") else {
            return;
        };
        let Ok(prop_name) = prop.utf8_text(source_bytes) else {
            return;
        };
        if prop_name != "offset" {
            return;
        }
        let Some(object) = function.child_by_field_name("object") else {
            return;
        };
        if !subtree_has_query_call(object, source_bytes) {
            return;
        }

        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "OFFSET pagination is O(N) — use cursor-based \
                      pagination with `.where(gt(col, cursor)).limit(N)` \
                      instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Returns true if `node` (or any descendant) contains a call whose method
/// is `from` or `select` — i.e. a Drizzle query builder chain.
fn subtree_has_query_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "call_expression" {
        if let Some(function) = node.child_by_field_name("function") {
            if function.kind() == "member_expression" {
                if let Some(prop) = function.child_by_field_name("property") {
                    if let Ok(name) = prop.utf8_text(source) {
                        if name == "from" || name == "select" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if subtree_has_query_call(child, source) {
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_drizzle_offset_pagination() {
        let src = "await db.select().from(users).offset(20).limit(10);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_cursor_pagination() {
        let src = "await db.select().from(users).where(gt(users.id, cursor)).limit(10);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_offset_outside_query() {
        let src = "arr.offset(5);";
        assert!(run(src).is_empty());
    }
}
