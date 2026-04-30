//! react-no-object-type-as-default-prop AST backend.
//!
//! Flags destructured function parameters with `= []`, `= {}`, or
//! `= () =>` defaults in React component declarations. These create a
//! new reference on every render, defeating `React.memo`.

use crate::diagnostic::{Diagnostic, Severity};

/// Return true when the node is a component-like function (name starts
/// with an uppercase ASCII letter).
fn is_component_fn(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "function_declaration" => {
            let Some(name) = node.child_by_field_name("name") else {
                return false;
            };
            let Ok(t) = name.utf8_text(source) else {
                return false;
            };
            t.starts_with(|c: char| c.is_ascii_uppercase())
        }
        "lexical_declaration" => {
            // `const Foo = (...) => { ... }`
            let Ok(t) = node.utf8_text(source) else {
                return false;
            };
            // Find the identifier after const/let
            let rest = t
                .strip_prefix("const ")
                .or_else(|| t.strip_prefix("let "))
                .or_else(|| t.strip_prefix("export const "))
                .or_else(|| t.strip_prefix("export default const "))
                .unwrap_or("");
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            name.starts_with(|c: char| c.is_ascii_uppercase())
                && (t.contains("=>") || t.contains("function"))
        }
        "export_statement" => {
            // Check the declaration inside the export
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .any(|child| is_component_fn(child, source))
        }
        _ => false,
    }
}

/// Check whether a destructuring pattern node contains `= {}`, `= []`,
/// or `= () =>` defaults.
fn has_object_defaults(node: tree_sitter::Node, _source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        // Shorthand property with default: `name = []`
        // In tree-sitter these appear as assignment_pattern or
        // object_assignment_pattern
        if child.kind().contains("assignment_pattern") {
            let Some(right) = child.child_by_field_name("right") else {
                continue;
            };
            match right.kind() {
                "object" | "array" | "arrow_function" => return true,
                _ => {}
            }
        }
        // Recurse into nested patterns
        if has_object_defaults(child, _source) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["function_declaration", "arrow_function", "lexical_declaration", "export_statement"] => |node, source, ctx, diagnostics|
    // We want function declarations or arrow functions that look like
    // React components and have destructured params with object defaults.
    let is_fn_decl = node.kind() == "function_declaration";
    let is_arrow = node.kind() == "arrow_function";
    let is_lex = node.kind() == "lexical_declaration";
    let is_export = node.kind() == "export_statement";

    // For lexical_declaration and export_statement, delegate to
    // is_component_fn which recurses. For function_declaration and
    // arrow_function, check directly.
    if (is_lex || is_export)
        && !is_component_fn(node, source)
    {
        return;
    }

    if is_fn_decl {
        let Some(name) = node.child_by_field_name("name") else { return };
        let Ok(t) = name.utf8_text(source) else { return };
        if !t.starts_with(|c: char| c.is_ascii_uppercase()) {
            return;
        }
    }

    // For arrow functions, check if parent is a variable_declarator
    // with an uppercase name.
    if is_arrow {
        let Some(parent) = node.parent() else { return };
        if parent.kind() != "variable_declarator" { return; }
        let Some(name) = parent.child_by_field_name("name") else { return };
        let Ok(t) = name.utf8_text(source) else { return };
        if !t.starts_with(|c: char| c.is_ascii_uppercase()) {
            return;
        }
    }

    // Find the formal_parameters → object_pattern with defaults.
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if current.kind() == "object_pattern" {
            if has_object_defaults(current, source) {
                let pos = current.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "react-no-object-type-as-default-prop".into(),
                    message: "Object/array/function default prop creates a new \
                              reference every render, breaking `React.memo`. Move \
                              the default to a module-level constant."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            return;
        }
        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_empty_array_default() {
        let src = "function Foo({ items = [] }) { return <div />; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_empty_object_default() {
        let src = "function Bar({ config = {} }) { return <div />; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_arrow_fn_default() {
        let src = "function Baz({ onClick = () => {} }) { return <div />; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_primitive_default() {
        let src = "function Foo({ count = 0, name = 'hello' }) { return <div />; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_component() {
        let src = "function helper({ items = [] }) { return items; }";
        assert!(run(src).is_empty());
    }
}
