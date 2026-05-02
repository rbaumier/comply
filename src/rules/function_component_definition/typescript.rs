//! function-component-definition AST backend.
//!
//! A React component is detected by two signals together: a PascalCase
//! binding name, and an arrow function body that produces JSX. When both
//! are present the binding should use a `function` declaration so the
//! component has a real name in stack traces and dev tools.
//!
//! Wrapping calls like `React.memo(() => <JSX />)` are skipped: the
//! surrounding call provides the component identity, not the arrow.

use crate::diagnostic::{Diagnostic, Severity};

fn is_test_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.")
        || s.contains(".spec.")
        || s.contains("__tests__")
        || s.contains("/tests/")
        || s.contains("\\tests\\")
}

crate::ast_check! { on ["variable_declarator"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir || is_test_path(ctx.path) {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else { return };
    if name_node.kind() != "identifier" {
        return;
    }
    let name_bytes = &source[name_node.byte_range()];
    if !starts_with_uppercase(name_bytes) {
        return;
    }

    let Some(value_node) = node.child_by_field_name("value") else { return };
    if value_node.kind() != "arrow_function" {
        return;
    }

    if !contains_jsx(value_node) {
        return;
    }

    let name_str = std::str::from_utf8(name_bytes).unwrap_or("component");
    let pos = value_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "React component `{name_str}` should be a `function` declaration, not an arrow function."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

fn starts_with_uppercase(bytes: &[u8]) -> bool {
    matches!(bytes.first(), Some(c) if c.is_ascii_uppercase())
}

fn contains_jsx(root: tree_sitter::Node) -> bool {
    let mut cursor = root.walk();
    let mut progressed = cursor.goto_first_child();
    while progressed {
        let child = cursor.node();
        if matches!(child.kind(), "jsx_element" | "jsx_self_closing_element") {
            return true;
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                progressed = false;
                break;
            }
            if cursor.node().id() == root.id() {
                progressed = false;
                break;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_arrow_component_self_closing() {
        let src = "const MyComponent = () => <div />;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_arrow_component_with_block_body() {
        let src = "const MyComponent = (props) => { return <div>{props.x}</div>; };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_exported_arrow_component() {
        let src = "export const MyComponent = () => <div />;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_function_declaration_component() {
        let src = "function MyComponent() { return <div />; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_pascal_arrow() {
        let src = "const handler = () => <div />;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_arrow_without_jsx() {
        let src = "const myUtil = () => someValue;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_pascal_arrow_without_jsx() {
        let src = "const MyThing = () => someValue;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_inline_test_component() {
        let src = "it('works', () => { const Component = () => <div />; render(<Component />); });";
        let d = crate::rules::test_helpers::run_tsx_with_project_file_and_path(
            src,
            &Check,
            crate::project::default_static_project_ctx(),
            crate::rules::file_ctx::default_static_file_ctx(),
            "component.test.tsx",
        );
        assert!(d.is_empty());
    }
}
