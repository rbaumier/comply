//! react-jsx-pascal-case AST backend.
//!
//! Flags JSX components whose name is not PascalCase.
//! HTML intrinsic elements (all-lowercase) are ignored.

use crate::diagnostic::{Diagnostic, Severity};

fn is_pascal_case(name: &str) -> bool {
    // Allow namespaced (Foo.Bar) — check each segment.
    for segment in name.split('.') {
        if segment.is_empty() {
            return false;
        }
        let first = segment.chars().next().unwrap();
        // Must start with uppercase.
        if !first.is_ascii_uppercase() {
            return false;
        }
        // Must not contain underscores or hyphens (SCREAMING_CASE, kebab).
        if segment.contains('_') || segment.contains('-') {
            return false;
        }
    }
    true
}

fn is_intrinsic(name: &str) -> bool {
    // HTML/SVG intrinsic elements are all-lowercase or contain hyphens (web components).
    let first = name.chars().next().unwrap_or('a');
    first.is_ascii_lowercase()
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_self_closing_element" && node.kind() != "jsx_opening_element" {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };

    // Skip intrinsic HTML elements.
    if is_intrinsic(tag) {
        return;
    }

    if !is_pascal_case(tag) {
        let pos = name_node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-jsx-pascal-case".into(),
            message: format!(
                "Component `{tag}` is not PascalCase — rename to PascalCase."
            ),
            severity: Severity::Warning,
            span: None,
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
    fn flags_non_pascal_case_component() {
        let src = "const x = <MY_COMPONENT />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_pascal_case() {
        let src = "const x = <MyComponent />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_namespaced_pascal() {
        let src = "const x = <Foo.Bar />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_html_elements() {
        let src = "const x = <div>hello</div>;";
        assert!(run(src).is_empty());
    }
}
