//! In files that reference `deletedAt`, flag `.findMany(` or `.select()`
//! call chains that do not include `isNull(` anywhere in the chain.

use crate::diagnostic::{Diagnostic, Severity};

/// Walk up through chained `.foo()` calls starting from `start` to find the
/// outermost call in the chain.
fn chain_root(start: tree_sitter::Node<'_>) -> tree_sitter::Node<'_> {
    let mut cur = start;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "member_expression" => {
                cur = parent;
            }
            "call_expression" => {
                cur = parent;
            }
            _ => break,
        }
    }
    cur
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !ctx.source.contains("deletedAt") {
        return;
    }
    if node.kind() != "call_expression" {
        return;
    }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }
    let Some(prop) = func.child_by_field_name("property") else { return };
    let name = prop.utf8_text(source).unwrap_or("");
    if name != "findMany" && name != "select" {
        return;
    }
    // Only flag the first call in a chain; subsequent `.where(...)` would be
    // processed via the root. We want to evaluate the full chain text.
    let root = chain_root(node);
    let chain_text = root.utf8_text(source).unwrap_or("");
    if chain_text.contains("isNull(") {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "`.{name}(...)` on a soft-deletable table without `isNull(t.deletedAt)` — add the filter or use a dedicated non-deleted helper."
        ),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_findmany_without_isnull() {
        let src = "const t = { deletedAt };\nconst r = db.query.users.findMany({})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_findmany_with_isnull() {
        let src = "const t = { deletedAt };\nconst r = db.query.users.findMany({ where: isNull(users.deletedAt) })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_files_without_deleted_at() {
        let src = "const r = db.query.users.findMany({})";
        assert!(run(src).is_empty());
    }
}
