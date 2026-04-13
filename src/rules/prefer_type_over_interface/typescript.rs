//! prefer-type-over-interface backend — default to `type`, use `interface`
//! only when you need extension or declaration merging.
//!
//! Why: the skill rule is "types by default, interface only for extension/perf".
//! `type` supports unions, intersections, mapped types, and conditional
//! types — `interface` doesn't. Using `type` everywhere keeps the toolkit
//! uniform. `interface` is still fine when you need `extends` for structural
//! inheritance or `declare module` augmentation.
//!
//! Detection: walk `interface_declaration` nodes and flag those WITHOUT an
//! `extends_type_clause` child. Interfaces that extend are allowed; the
//! extension is the only feature `type` doesn't do cleanly.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
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
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-type-over-interface".into(),
                message: format!(
                    "Interface '{name}' has no extends clause — use \
                     `type {name} = {{ ... }}` instead. Types support \
                     unions, intersections, mapped types, and conditional \
                     types. Keep `interface` for extension and declaration \
                     merging only."
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
}
