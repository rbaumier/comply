//! react-hoist-static-jsx backend — flag `const x = <static-jsx />`
//! declarations inside a component body when the JSX tree has no dynamic
//! content.
//!
//! Why: every render rebuilds the JSX element tree, forcing React to
//! reconcile identical nodes and defeating `React.memo` on consumers that
//! receive it as a prop. Hoisted to module scope, the element is built once.
//!
//! Staticness heuristic: a JSX subtree is "static" when it contains
//! - no `jsx_expression` nodes (the `{...}` interpolation),
//! - no tags whose name starts with an uppercase letter (custom components
//!   can close over module state the author expects to re-evaluate).
//!
//! Scope: only fires on `const name = <jsx />` (lexical_declaration with a
//! jsx_element / jsx_self_closing_element initializer) located inside a
//! PascalCase function declaration or an arrow/function_expression assigned
//! to a PascalCase variable.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Recursive: true when `node` (and any descendant) has no `jsx_expression`
/// and no uppercase-named JSX tags.
fn is_static_jsx(node: tree_sitter::Node, source: &[u8]) -> bool {
    let kind = node.kind();
    if kind == "jsx_expression" {
        return false;
    }
    if (kind == "jsx_self_closing_element" || kind == "jsx_opening_element")
        && let Some(name_node) = node.child_by_field_name("name")
        && let Ok(name) = name_node.utf8_text(source)
        && starts_with_uppercase(name)
    {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !is_static_jsx(child, source) {
            return false;
        }
    }
    true
}

/// True when `node` lives inside a PascalCase component function body.
fn inside_component_body(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "function_declaration" => {
                if let Some(name_node) = parent.child_by_field_name("name")
                    && let Ok(name) = name_node.utf8_text(source)
                    && starts_with_uppercase(name)
                {
                    return true;
                }
            }
            "variable_declarator" => {
                // Arrow or function_expression assigned to a PascalCase
                // variable counts as a component body.
                if let Some(name_node) = parent.child_by_field_name("name")
                    && let Ok(name) = name_node.utf8_text(source)
                    && starts_with_uppercase(name)
                {
                    let mut cursor = parent.walk();
                    if parent
                        .children(&mut cursor)
                        .any(|c| matches!(c.kind(), "arrow_function" | "function_expression"))
                    {
                        return true;
                    }
                }
            }
            _ => {}
        }
        current = parent;
    }
    false
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["variable_declarator"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        // Match `const x = <...>` or `let x = <...>`.
        let Some(value) = node.child_by_field_name("value") else {
            return;
        };
        if !matches!(value.kind(), "jsx_element" | "jsx_self_closing_element") {
            return;
        }
        if !inside_component_body(node, source) {
            return;
        }
        if !is_static_jsx(value, source) {
            return;
        }

        let pos = value.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-hoist-static-jsx".into(),
            message: "Static JSX inside a component is rebuilt every render. \
                      Move this element to a module-level `const` above the \
                      component so it's built once."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_static_jsx_in_component() {
        let src = r#"
function Page() {
    const icon = <svg width="16" height="16" />;
    return <div>{icon}</div>;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_static_jsx_with_children() {
        let src = r#"
function Header() {
    const title = <h1 className="big">Welcome</h1>;
    return <header>{title}</header>;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_dynamic_jsx() {
        let src = r#"
function Page({ name }: { name: string }) {
    const greeting = <span>{name}</span>;
    return <div>{greeting}</div>;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_jsx_with_custom_component() {
        // `MyButton` could close over module state; don't flag.
        let src = r#"
function Page() {
    const btn = <MyButton label="go" />;
    return <div>{btn}</div>;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_module_level_static_jsx() {
        let src = r#"
const icon = <svg width="16" />;
function Page() { return <div>{icon}</div>; }
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_component_function() {
        // lowercase `helper` — not a component.
        let src = r#"
function helper() {
    const icon = <svg />;
    return icon;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_inside_arrow_component() {
        let src = r#"
const Page = () => {
    const icon = <svg width="16" />;
    return <div>{icon}</div>;
};
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
