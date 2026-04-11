//! react-jsx-key AST backend.
//!
//! Flags JSX elements inside `.map()` / `.flatMap()` callbacks and array
//! literals that lack a `key` prop.

use crate::diagnostic::{Diagnostic, Severity};

fn has_key_prop(node: tree_sitter::Node, source: &[u8]) -> bool {
    // For jsx_element, check the opening element.
    // For jsx_self_closing_element, check the node itself.
    let tag_node = if node.kind() == "jsx_element" {
        let Some(opening) = node.child(0) else { return false };
        if opening.kind() != "jsx_opening_element" { return false; }
        opening
    } else {
        node
    };

    let mut cursor = tag_node.walk();
    tag_node.children(&mut cursor).any(|child| {
        if child.kind() != "jsx_attribute" {
            return false;
        }
        let Some(attr_name) = child.child(0) else { return false };
        let Ok(name_text) = attr_name.utf8_text(source) else { return false };
        name_text == "key"
    })
}

fn is_in_iterator(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut parent = node.parent();

    while let Some(p) = parent {
        match p.kind() {
            "array" => return true,
            "parenthesized_expression" | "jsx_expression" | "return_statement"
            | "expression_statement" => {
                // Transparent wrappers — keep walking.
            }
            "arrow_function" | "function_expression" | "function_declaration" | "function" => {
                // Check if the parent of this function is a .map()/.flatMap() call.
                if let Some(args) = p.parent()
                    && args.kind() == "arguments"
                        && let Some(call_expr) = args.parent()
                            && call_expr.kind() == "call_expression"
                                && let Some(fn_node) = call_expr.child(0)
                                    && fn_node.kind() == "member_expression"
                                        && let Some(prop) = fn_node.child_by_field_name("property") {
                                            let Ok(method) = prop.utf8_text(source) else { return false };
                                            if matches!(method, "map" | "flatMap" | "from") {
                                                return true;
                                            }
                                        }
                return false;
            }
            _ => return false,
        }
        parent = p.parent();
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let is_jsx = node.kind() == "jsx_self_closing_element" || node.kind() == "jsx_element";

    if !is_jsx {
        return;
    }

    if has_key_prop(node, source) {
        return;
    }

    if is_in_iterator(node, source) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-jsx-key".into(),
            message: "Missing `key` prop for JSX element in iterator — \
                      React needs stable keys to reconcile lists."
                .into(),
            severity: Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_map_without_key() {
        let src = "const x = items.map(i => <li>{i}</li>);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_map_with_key() {
        let src = "const x = items.map(i => <li key={i.id}>{i}</li>);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_array_literal_without_key() {
        let src = "const x = [<div>a</div>, <div>b</div>];";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_standalone_element() {
        let src = "const x = <div>hello</div>;";
        assert!(run(src).is_empty());
    }
}
