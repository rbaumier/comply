//! ts-no-unsafe-declaration-merging backend — collect class and interface names,
//! flag overlaps.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let source = ctx.source.as_bytes();
        let mut class_names: Vec<(String, usize, usize)> = Vec::new();
        let mut interface_names: Vec<(String, usize, usize)> = Vec::new();

        walk_tree(tree, |node| {
            let (kind, field) = match node.kind() {
                "class_declaration" => ("class", "name"),
                "interface_declaration" => ("interface", "name"),
                _ => return,
            };
            let Some(name_node) = node.child_by_field_name(field) else {
                return;
            };
            let name = &source[name_node.byte_range()];
            let Ok(name_str) = std::str::from_utf8(name) else {
                return;
            };
            let pos = name_node.start_position();
            let entry = (name_str.to_string(), pos.row + 1, pos.column + 1);
            if kind == "class" {
                class_names.push(entry);
            } else {
                interface_names.push(entry);
            }
        });

        // Flag interfaces that share a name with a class
        for (iface_name, line, col) in &interface_names {
            if class_names.iter().any(|(c, _, _)| c == iface_name) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: *line,
                    column: *col,
                    rule_id: "ts-no-unsafe-declaration-merging".into(),
                    message: format!(
                        "Unsafe declaration merging — interface `{iface_name}` \
                         shares a name with a class."
                    ),
                    severity: Severity::Warning,
                });
            }
        }
        // Also flag classes that share a name with an interface
        for (class_name, line, col) in &class_names {
            if interface_names.iter().any(|(i, _, _)| i == class_name) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: *line,
                    column: *col,
                    rule_id: "ts-no-unsafe-declaration-merging".into(),
                    message: format!(
                        "Unsafe declaration merging — class `{class_name}` \
                         shares a name with an interface."
                    ),
                    severity: Severity::Warning,
                });
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_class_and_interface_same_name() {
        let diags = run_on("interface Foo {} class Foo {}");
        assert_eq!(diags.len(), 2); // one for each declaration
    }

    #[test]
    fn allows_different_names() {
        assert!(run_on("interface Foo {} class Bar {}").is_empty());
    }

    #[test]
    fn allows_class_only() {
        assert!(run_on("class Foo {}").is_empty());
    }

    #[test]
    fn allows_interface_only() {
        assert!(run_on("interface Foo { x: number }").is_empty());
    }
}
