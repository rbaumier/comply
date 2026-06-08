//! drizzle-relations-missing-inverse — for every `relations(<table>, ...)`
//! call in the file, collect the tables referenced via `one(<other>, ...)` /
//! `many(<other>, ...)` inside the callback. Flag any referenced table for
//! which the file does not also contain a `relations(<other>, ...)` call.

use crate::diagnostic::{Diagnostic, Severity};
use rustc_hash::FxHashSet;

fn declared_relation_tables(root: tree_sitter::Node<'_>, source: &[u8]) -> FxHashSet<String> {
    let mut declared = FxHashSet::default();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression" {
            if let Some(callee) = node.child_by_field_name("function") {
                if callee.utf8_text(source).unwrap_or("") == "relations" {
                    if let Some(args) = node.child_by_field_name("arguments") {
                        let mut cursor = args.walk();
                        if let Some(first) = args.named_children(&mut cursor).next() {
                            if first.kind() == "identifier" {
                                declared.insert(first.utf8_text(source).unwrap_or("").to_string());
                            }
                        }
                    }
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    declared
}

fn referenced_tables<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Vec<(String, tree_sitter::Node<'a>)> {
    let mut refs = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression" {
            if let Some(callee) = n.child_by_field_name("function") {
                let name = callee.utf8_text(source).unwrap_or("");
                if name == "one" || name == "many" {
                    if let Some(args) = n.child_by_field_name("arguments") {
                        let mut cursor = args.walk();
                        if let Some(first) = args.named_children(&mut cursor).next() {
                            if first.kind() == "identifier" {
                                refs.push((first.utf8_text(source).unwrap_or("").to_string(), n));
                            }
                        }
                    }
                }
            }
        }
        let mut cursor = n.walk();
        for child in n.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    refs
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.utf8_text(source).unwrap_or("") != "relations" {
        return;
    }

    // Only operate at top-level relations() calls, not at nested ones.
    // Climb up to program root.
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let declared = declared_relation_tables(root, source);

    let refs = referenced_tables(node, source);
    let mut seen: FxHashSet<&str> = FxHashSet::default();
    for (name, ref_node) in &refs {
        if !seen.insert(name.as_str()) {
            continue;
        }
        if declared.contains(name) {
            continue;
        }
        let pos = ref_node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "drizzle-relations-missing-inverse".into(),
            message: format!("`relations(...)` references `{}` but no inverse `relations({}, ...)` is defined in this file.", name, name),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_one_without_inverse() {
        let src = "export const usersRelations = relations(users, ({ one }) => ({ profile: one(profiles) }));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_inverse_present() {
        let src = "export const usersRelations = relations(users, ({ one }) => ({ profile: one(profiles) }));\nexport const profilesRelations = relations(profiles, ({ one }) => ({ user: one(users) }));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_many_without_inverse() {
        let src = "export const usersRelations = relations(users, ({ many }) => ({ posts: many(posts) }));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_call_outside_relations() {
        let src = "function one() {}\nfunction many() {}\nconst x = one(profiles);";
        assert!(run(src).is_empty());
    }
}
