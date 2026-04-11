//! react-no-this-in-sfc AST backend.
//!
//! Detects `this.` inside functional components. Functional components
//! use hooks, not `this.state` / `this.props`.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a node subtree contains any JSX nodes.
fn subtree_has_jsx(node: tree_sitter::Node) -> bool {
    match node.kind() {
        "jsx_element" | "jsx_self_closing_element" | "jsx_fragment" => true,
        _ => {
            let mut cursor = node.walk();
            node.children(&mut cursor).any(|child| subtree_has_jsx(child))
        }
    }
}

/// Check if a node subtree contains `this.` member expressions.
fn find_this_usages(node: tree_sitter::Node, _source: &[u8], positions: &mut Vec<(usize, usize)>) {
    if node.kind() == "member_expression"
        && let Some(obj) = node.child_by_field_name("object")
        && obj.kind() == "this"
    {
        let pos = node.start_position();
        positions.push((pos.row + 1, pos.column + 1));
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        find_this_usages(child, _source, positions);
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Match function declarations with PascalCase names (functional components).
    let is_fn_decl = node.kind() == "function_declaration";
    let is_arrow = node.kind() == "arrow_function";

    if !is_fn_decl && !is_arrow {
        return;
    }

    // Check if it's a component (PascalCase name).
    if is_fn_decl {
        let Some(name) = node.child_by_field_name("name") else { return };
        let Ok(t) = name.utf8_text(source) else { return };
        if !t.starts_with(|c: char| c.is_ascii_uppercase()) {
            return;
        }
    }

    if is_arrow {
        let Some(parent) = node.parent() else { return };
        if parent.kind() != "variable_declarator" { return; }
        let Some(name) = parent.child_by_field_name("name") else { return };
        let Ok(t) = name.utf8_text(source) else { return };
        if !t.starts_with(|c: char| c.is_ascii_uppercase()) {
            return;
        }
    }

    // Must not be inside a class (skip class methods — those are class components).
    let mut ancestor = node.parent();
    while let Some(a) = ancestor {
        if a.kind() == "class_body" || a.kind() == "class_declaration" {
            return;
        }
        ancestor = a.parent();
    }

    // Check if the function body contains JSX.
    if !subtree_has_jsx(node) {
        return;
    }

    // Find `this.` usages.
    let mut positions = Vec::new();
    find_this_usages(node, source, &mut positions);

    for (line, column) in positions {
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line,
            column,
            rule_id: "react-no-this-in-sfc".into(),
            message: "`this` has no meaning in a functional component. \
                      Use hooks instead."
                .into(),
            severity: Severity::Error,
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
    fn flags_this_in_functional_component() {
        let src = r#"
function MyComponent() {
    const value = this.props.name;
    return <div>{value}</div>;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_this_state_in_functional() {
        let src = r#"
function Counter() {
    const count = this.state.count;
    return <span>{count}</span>;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_this_in_class_component() {
        let src = r#"
class MyComponent extends React.Component {
    render() {
        return <div>{this.props.name}</div>;
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_functional_without_this() {
        let src = r#"
function MyComponent({ name }) {
    return <div>{name}</div>;
}
"#;
        assert!(run(src).is_empty());
    }
}
