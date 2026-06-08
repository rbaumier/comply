//! sql-no-union-when-union-all — Drizzle ORM backend.
//!
//! Flags `.union(...)` calls (not `.unionAll(...)`) on a Drizzle query
//! builder chain. We confirm the call is part of a query by walking the
//! receiver subtree for a `.from(...)` or `.select(...)` call.

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
        if prop_name != "union" {
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
            message: "`.union()` deduplicates rows — use `.unionAll()` when \
                      rows are already unique."
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
    fn flags_drizzle_union() {
        let src = "const q = db.select().from(a).union(db.select().from(b));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_union_all() {
        let src = "const q = db.select().from(a).unionAll(db.select().from(b));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_union_outside_query() {
        let src = "set.union(otherSet);";
        assert!(run(src).is_empty());
    }
}
