//! sql-no-select-star — Drizzle ORM backend.
//!
//! Flags `db.select()` (with zero arguments) followed by `.from(...)` —
//! Drizzle treats an empty `select()` as "all columns", which is the same
//! footgun as `SELECT *`.

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
        if prop_name != "select" {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        if args.named_child_count() != 0 {
            return;
        }
        if !chain_has_from(node, source_bytes) {
            return;
        }

        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "`db.select()` without columns selects all fields — \
                      list columns explicitly with \
                      `db.select({ id: table.id, name: table.name })`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Returns true if the chain rooted at `start` (a `select()` call) is
/// followed by a `.from(...)` call somewhere up the chain.
fn chain_has_from(start: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = start;
    while let Some(parent) = current.parent() {
        if parent.kind() == "member_expression"
            && parent.child_by_field_name("object").map(|o| o.id()) == Some(current.id())
        {
            let Some(grand) = parent.parent() else { break };
            if grand.kind() == "call_expression"
                && grand.child_by_field_name("function").map(|f| f.id()) == Some(parent.id())
            {
                if let Some(prop) = parent.child_by_field_name("property") {
                    if prop.utf8_text(source).unwrap_or("") == "from" {
                        return true;
                    }
                }
                current = grand;
                continue;
            }
        }
        break;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_empty_select_with_from() {
        let src = "await db.select().from(users);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_explicit_columns() {
        let src = "await db.select({ id: users.id }).from(users);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_select_outside_query() {
        let src = "arr.select();";
        assert!(run(src).is_empty());
    }
}
