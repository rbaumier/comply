//! a11y-no-noninteractive-element-to-interactive-role AST backend.

use crate::diagnostic::{Diagnostic, Severity};

const NON_INTERACTIVE_ELEMENTS: &[&str] = &[
    "div", "span", "p", "section", "article", "header", "footer",
];

const INTERACTIVE_ROLES: &[&str] = &[
    "button", "link", "checkbox", "radio", "tab", "switch",
    "menuitem", "option", "textbox", "combobox", "searchbox",
    "spinbutton", "slider",
];

/// Extract the string value from a JSX attribute value node.
fn attr_string_value<'a>(attr: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    crate::rules::jsx::jsx_attribute_string_value(attr, source)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_opening_element" && node.kind() != "jsx_self_closing_element" {
        return;
    }

    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };

    if !NON_INTERACTIVE_ELEMENTS.contains(&tag) {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name) = attr_name.utf8_text(source) else { continue };
        if name != "role" {
            continue;
        }
        if let Some(role) = attr_string_value(child, source)
            && INTERACTIVE_ROLES.contains(&role) {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "a11y-no-noninteractive-element-to-interactive-role".into(),
                    message: format!(
                        "Non-interactive element should not have interactive `role=\"{role}\"`."
                    ),
                    severity: Severity::Warning,
                });
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
    fn flags_div_with_button_role() {
        assert_eq!(run(r#"const x = <div role="button">Click me</div>;"#).len(), 1);
    }

    #[test]
    fn flags_span_with_link_role() {
        assert_eq!(run(r#"const x = <span role="link">Go</span>;"#).len(), 1);
    }

    #[test]
    fn allows_div_with_noninteractive_role() {
        assert!(run(r#"const x = <div role="article">Content</div>;"#).is_empty());
    }

    #[test]
    fn allows_button_element() {
        assert!(run(r#"const x = <button role="button">X</button>;"#).is_empty());
    }
}
