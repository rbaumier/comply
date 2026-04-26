//! prefer-set-has backend — flag `const arr = [...]; arr.includes(x)` patterns.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    // We only run at the program (root) level to do a two-pass scan.
    // Phase 1: collect names of `const NAME = [...]` declarations.
    let mut array_names = HashSet::new();
    collect_const_arrays(node, source, &mut array_names);

    if array_names.is_empty() {
        return;
    }

    // Phase 2: find `.includes(` calls on those names.
    find_includes_calls(node, source, ctx, &array_names, diagnostics);
}

fn collect_const_arrays<'a>(
    node: tree_sitter::Node<'a>,
    source: &[u8],
    names: &mut HashSet<String>,
) {
    // Look for variable_declarator with kind=const and value=array.
    // tree-sitter uses `lexical_declaration` for const/let.
    if node.kind() == "lexical_declaration" || node.kind() == "variable_declaration" {
        // Check if it's a `const` declaration
        let text = node.utf8_text(source).unwrap_or("");
        if !text.starts_with("const") {
            return;
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                let Some(name_node) = child.child_by_field_name("name") else { continue };
                let Some(value_node) = child.child_by_field_name("value") else { continue };
                if value_node.kind() == "array"
                    && let Ok(name) = name_node.utf8_text(source) {
                        names.insert(name.to_owned());
                    }
            }
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_const_arrays(child, source, names);
    }
}

fn find_includes_calls(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    array_names: &HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if node.kind() == "call_expression"
        && let Some(func) = node.child_by_field_name("function")
            && func.kind() == "member_expression" {
                let obj = func.child_by_field_name("object");
                let prop = func.child_by_field_name("property");
                if let (Some(obj_node), Some(prop_node)) = (obj, prop) {
                    let prop_text = prop_node.utf8_text(source).unwrap_or("");
                    if prop_text == "includes" {
                        let obj_text = obj_node.utf8_text(source).unwrap_or("");
                        if array_names.contains(obj_text) {
                            let pos = node.start_position();
                            diagnostics.push(Diagnostic {
                                path: std::sync::Arc::clone(&ctx.path_arc),
                                line: pos.row + 1,
                                column: pos.column + 1,
                                rule_id: "prefer-set-has".into(),
                                message: format!(
                                    "`{obj_text}` is a const array used with `.includes()` — consider using a `Set` with `.has()` for O(1) lookups."
                                ),
                                severity: Severity::Warning,
                                span: None,
                            });
                        }
                    }
                }
            }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        find_includes_calls(child, source, ctx, array_names, diagnostics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_const_array_with_includes() {
        let source = "\
const items = [1, 2, 3];
for (const x of data) {
  if (items.includes(x)) {}
}";
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("items"));
        assert!(d[0].message.contains("Set"));
    }

    #[test]
    fn flags_multiple_includes_calls() {
        let source = "\
const allowed = ['a', 'b', 'c'];
allowed.includes(x);
allowed.includes(y);";
        let d = run_on(source);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn allows_let_array_with_includes() {
        let source = "\
let items = [1, 2, 3];
items.includes(1);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_no_includes_call() {
        let source = "const items = [1, 2, 3];\nconsole.log(items);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_set_has() {
        let source = "\
const items = new Set([1, 2, 3]);
items.has(1);";
        assert!(run_on(source).is_empty());
    }
}
