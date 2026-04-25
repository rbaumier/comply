//! In files where `deletedAt` is declared as a column on a Drizzle table
//! schema (e.g. `deletedAt: timestamp(...)`, `deletedAt: integer(...)`,
//! `deletedAt: text(...)`), flag `.findMany(` or `.select()` call chains
//! that do not include `isNull(` anywhere in the chain.
//!
//! A bare textual mention of `deletedAt` (e.g. an object spread, a comment,
//! or a derived selector) is NOT enough — we want evidence the table itself
//! supports soft-deletion before warning about the missing filter.

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

/// Drizzle column constructors that, when the right-hand side of a
/// `deletedAt:` pair calls one of them, prove the file declares a real
/// soft-delete column.
const COLUMN_CTORS: &[&str] = &[
    "timestamp",
    "timestamptz",
    "datetime",
    "date",
    "integer",
    "int",
    "bigint",
    "text",
    "varchar",
    "char",
    "boolean",
    "bool",
];

/// True when the file contains a property pair of the form
/// `deletedAt: <columnCtor>(...)` somewhere in the AST — that's the
/// canonical Drizzle table column definition shape.
fn file_has_deleted_at_column(program: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cursor = program.walk();
    let root_id = program.id();
    loop {
        let n = cursor.node();
        if n.kind() == "pair"
            && let Some(key) = n.child_by_field_name("key")
            && let Ok(key_text) = std::str::from_utf8(&source[key.byte_range()])
            && key_text.trim_matches(|c| c == '"' || c == '\'') == "deletedAt"
            && let Some(value) = n.child_by_field_name("value")
            && value.kind() == "call_expression"
            && let Some(func) = value.child_by_field_name("function")
        {
            let func_text = std::str::from_utf8(&source[func.byte_range()]).unwrap_or("");
            // `timestamp(...)`, `pg.timestamp(...)`, `t.timestamp(...)`, etc.
            let bare = func_text.rsplit('.').next().unwrap_or(func_text);
            if COLUMN_CTORS.contains(&bare) {
                return true;
            }
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.node().id() == root_id {
                return false;
            }
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return false;
            }
        }
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Cheap pre-filter: if the file doesn't even mention deletedAt, skip
    // the AST walk entirely.
    if !ctx.source.contains("deletedAt") {
        return;
    }
    // Run the column-definition check once per program-level invocation.
    // We piggy-back on `node`: traverse from its root.
    let mut root = node;
    while let Some(p) = root.parent() {
        root = p;
    }
    if !file_has_deleted_at_column(root, source) {
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
        let src = "export const users = pgTable('u', { id: text('id'), deletedAt: timestamp('deleted_at') });\n\
                   const r = db.query.users.findMany({})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_findmany_with_isnull() {
        let src = "export const users = pgTable('u', { id: text('id'), deletedAt: timestamp('deleted_at') });\n\
                   const r = db.query.users.findMany({ where: isNull(users.deletedAt) })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_files_without_deleted_at() {
        let src = "const r = db.query.users.findMany({})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_bare_deleted_at_mention_without_column_definition() {
        // REVIEW regression: a stray `deletedAt` reference (object shorthand,
        // comment, derived selector, etc.) is not proof that the table
        // supports soft-deletion. We must see a column definition.
        let src = "const t = { deletedAt };\nconst r = db.query.users.findMany({})";
        assert!(run(src).is_empty());
    }
}
