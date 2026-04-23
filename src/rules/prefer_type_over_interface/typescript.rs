//! prefer-type-over-interface backend — default to `type`, use `interface`
//! only when you need extension, declaration merging, or `implements`.
//!
//! Why: the skill rule is "types by default, interface only for extension/perf".
//! `type` supports unions, intersections, mapped types, and conditional
//! types — `interface` doesn't. Using `type` everywhere keeps the toolkit
//! uniform. `interface` is still fine when you need `extends` for structural
//! inheritance, `declare module` augmentation, or when a class `implements` it.
//!
//! Detection: walk `interface_declaration` nodes and flag those WITHOUT an
//! `extends_type_clause` child AND not used in an `implements` clause.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;
use std::collections::HashSet;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();

        // First pass: collect all interface names used in `implements` clauses.
        let mut implemented: HashSet<String> = HashSet::new();
        walk_tree(tree, |node| {
            if node.kind() == "implements_clause" {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if let Ok(name) = child.utf8_text(source_bytes) {
                        // Handle both simple identifiers and generic types
                        let base_name = name.split('<').next().unwrap_or(name).trim();
                        if !base_name.is_empty() && base_name != "implements" {
                            implemented.insert(base_name.to_string());
                        }
                    }
                }
            }
        });

        // Second pass: flag interfaces that don't extend and aren't implemented.
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "interface_declaration" {
                return;
            }
            if has_extends_clause(node) {
                return;
            }
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source_bytes).ok())
                .unwrap_or("<interface>");

            // Allow if interface is implemented by a class
            if implemented.contains(name) {
                return;
            }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-type-over-interface".into(),
                message: format!(
                    "Interface '{name}' has no extends clause and is not implemented — use \
                     `type {name} = {{ ... }}` instead. Types support \
                     unions, intersections, mapped types, and conditional \
                     types. Keep `interface` for extension, declaration \
                     merging, and `implements` only."
                ),
                severity: Severity::Warning,
                span: None,
            });
        });
        diagnostics
    }
}

fn has_extends_clause(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|c| c.kind() == "extends_type_clause")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_plain_interface() {
        assert_eq!(run_on("interface User { name: string; }").len(), 1);
    }

    #[test]
    fn allows_interface_with_extends() {
        assert!(run_on("interface Admin extends User { role: string; }").is_empty());
    }

    #[test]
    fn allows_type_alias() {
        assert!(run_on("type User = { name: string };").is_empty());
    }

    #[test]
    fn allows_interface_with_implements() {
        let code = r#"
            interface Serializable { serialize(): string; }
            class User implements Serializable { serialize() { return ""; } }
        "#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_interface_with_generic_implements() {
        let code = r#"
            interface Repository<T> { find(id: string): T; }
            class UserRepo implements Repository<User> { find(id: string) { return null; } }
        "#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn flags_interface_not_implemented() {
        let code = r#"
            interface Unused { foo: string; }
            class User implements OtherInterface {}
        "#;
        assert_eq!(run_on(code).len(), 1);
    }
}
