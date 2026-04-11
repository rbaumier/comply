//! react-self-closing-comp AST backend.
//!
//! Flags `<Foo></Foo>` or `<div></div>` when there are no children.

use crate::diagnostic::{Diagnostic, Severity};

/// HTML void elements that must always self-close (never flagged).
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link",
    "meta", "param", "source", "track", "wbr",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_element" {
        return;
    }

    // Must have opening + closing tag but no meaningful children.
    let child_count = node.child_count();
    if child_count < 2 {
        return;
    }

    let Some(opening) = node.child(0) else { return };
    if opening.kind() != "jsx_opening_element" {
        return;
    }

    // Get tag name.
    let Some(name_node) = opening.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };

    // Skip void elements — they always self-close in well-formed HTML.
    if VOID_ELEMENTS.contains(&tag) {
        return;
    }

    // Check if there are any children between opening and closing.
    let mut has_children = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "jsx_opening_element" | "jsx_closing_element" => continue,
            "jsx_text" => {
                let Ok(text) = child.utf8_text(source) else { continue };
                if !text.trim().is_empty() {
                    has_children = true;
                    break;
                }
            }
            _ => {
                has_children = true;
                break;
            }
        }
    }

    if !has_children {
        let pos = opening.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-self-closing-comp".into(),
            message: format!(
                "`<{tag}></{tag}>` has no children — use `<{tag} />` instead."
            ),
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
    fn flags_empty_component() {
        let src = "const x = <MyComponent></MyComponent>;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_empty_div() {
        let src = "const x = <div></div>;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_self_closing() {
        let src = "const x = <MyComponent />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_element_with_children() {
        let src = "const x = <div>Hello</div>;";
        assert!(run(src).is_empty());
    }
}
