//! Flag `createInsertSchema(...)` calls whose enclosing chain does not
//! include `.omit(`.

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

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "identifier" {
        return;
    }
    if func.utf8_text(source).unwrap_or("") != "createInsertSchema" {
        return;
    }
    let root = chain_root(node);
    let text = root.utf8_text(source).unwrap_or("");
    if text.contains(".omit(") {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`createInsertSchema(table)` must chain `.omit({ id: true, createdAt: true, ... })` so API consumers don't submit DB-generated columns.".into(),
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
}
