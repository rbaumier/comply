//! Flag `createInsertSchema(...)` calls whose enclosing chain either does
//! not include a `.omit({...})` step, or whose `.omit({...})` step omits
//! an object that doesn't drop `id` (the canonical DB-generated column).
//!
//! Naming the generated column is required: a `.omit({ name: true })`
//! that doesn't drop `id` still lets API consumers submit a primary key,
//! which is exactly the misuse we want to catch.

use crate::diagnostic::{Diagnostic, Severity};

fn chain_root(start: tree_sitter::Node<'_>) -> tree_sitter::Node<'_> {
    let mut cur = start;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "member_expression" | "call_expression" => cur = parent,
            _ => break,
        }
    }
    cur
}

/// Walk every `.omit(...)` call inside `root`. Return true if at least one
/// of them passes a single object argument that contains an `id` key.
fn chain_has_omit_with_id(root: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let root_id = root.id();
    let mut cursor = root.walk();
    loop {
        let n = cursor.node();
        if n.kind() == "call_expression"
            && let Some(func) = n.child_by_field_name("function")
            && func.kind() == "member_expression"
            && let Some(prop) = func.child_by_field_name("property")
            && prop.utf8_text(source).unwrap_or("") == "omit"
            && let Some(args) = n.child_by_field_name("arguments")
            && let Some(first) = args.named_child(0)
            && first.kind() == "object"
            && object_has_key(first, source, "id")
        {
            return true;
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

fn object_has_key(object: tree_sitter::Node<'_>, source: &[u8], needle: &str) -> bool {
    let mut cursor = object.walk();
    for child in object.named_children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let Some(key) = child.child_by_field_name("key") else {
            continue;
        };
        let Ok(raw) = key.utf8_text(source) else {
            continue;
        };
        if raw.trim_matches(|c| c == '"' || c == '\'') == needle {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "identifier" {
        return;
    }
    if func.utf8_text(source).unwrap_or("") != "createInsertSchema" {
        return;
    }
    let root = chain_root(node);
    let text = root.utf8_text(source).unwrap_or("");
    if !text.contains(".omit(") {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`createInsertSchema(table)` must chain `.omit({ id: true, createdAt: true, ... })` so API consumers don't submit DB-generated columns.".into(),
            Severity::Warning,
        ));
        return;
    }
    if chain_has_omit_with_id(root, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`createInsertSchema(table).omit(...)` must drop the generated `id` column at minimum.".into(),
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
    fn flags_create_insert_schema_without_omit() {
        let src = "export const schema = createInsertSchema(users)";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_create_insert_schema_with_omit() {
        let src = "export const schema = createInsertSchema(users).omit({ id: true, createdAt: true })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_other_calls() {
        let src = "export const schema = createSelectSchema(users)";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_omit_that_does_not_drop_id() {
        // REVIEW regression: an `.omit({ name: true })` that doesn't drop
        // `id` still allows clients to submit a primary key — flag it.
        let src = "export const schema = createInsertSchema(users).omit({ name: true })";
        assert_eq!(run(src).len(), 1);
    }
}
