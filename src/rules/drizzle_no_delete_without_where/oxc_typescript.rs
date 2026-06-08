//! drizzle-no-delete-without-where oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Walk up the parent chain of `node` (a CallExpression). Returns true
/// if any ancestor call's *method* is `target_method` and the chain is
/// continuous (each parent is a member-access on the previous).
fn chain_contains_method<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    target_method: &str,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::StaticMemberExpression(_) => {
                current_id = parent_id;
            }
            AstKind::CallExpression(call) => {
                if let Expression::StaticMemberExpression(m) = &call.callee
                    && m.property.name.as_str() == target_method
                {
                    return true;
                }
                current_id = parent_id;
            }
            _ => return false,
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".delete("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        // The current node must itself be `<x>.delete(<table>)`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "delete" {
            return;
        }
        // Require an argument (`db.delete(users)`) — bare `.delete()`
        // is more likely a Set/Map call.
        if call.arguments.is_empty() {
            return;
        }
        // The receiver `db` should look like a Drizzle handle — heuristic:
        // identifier `db`, `tx`, `transaction`, or `*Db` suffix.
        let receiver_ok = match &member.object {
            Expression::Identifier(id) => {
                let name = id.name.as_str();
                name == "db"
                    || name == "tx"
                    || name == "transaction"
                    || name.ends_with("Db")
                    || name.ends_with("Tx")
            }
            _ => false,
        };
        if !receiver_ok {
            return;
        }
        // OK — the chain must also contain `.where(...)` somewhere up
        // the parent chain, otherwise this is a bulk delete.
        if chain_contains_method(node, semantic, "where") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Bulk `.delete()` without `.where(...)` purges every row in \
                      the table. Add a `.where(...)` filter or, if a truncate is \
                      intended, call `db.execute(sql`TRUNCATE …`)` with a comment."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_delete_without_where() {
        let src = r#"const r = await db.delete(users);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_delete_with_where() {
        let src = r#"const r = await db.delete(users).where(eq(users.id, 1));"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_set_delete() {
        // Native Set.delete — receiver isn't a Drizzle-ish identifier.
        let src = r#"const set = new Set(); set.delete(x);"#;
        assert!(run(src).is_empty());
    }
}
