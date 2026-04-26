//! html-no-nested-interactive backend — flag nested interactive elements.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx;

const INTERACTIVE_ELEMENTS: &[&str] = &["button", "a", "input", "select", "textarea", "details"];

const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "link",
    "checkbox",
    "radio",
    "switch",
    "tab",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "combobox",
    "listbox",
    "slider",
    "spinbutton",
    "textbox",
    "searchbox",
    "treeitem",
];

fn get_element_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() == "jsx_self_closing_element" || node.kind() == "jsx_opening_element" {
        let name_node = node.child_by_field_name("name")?;
        return name_node.utf8_text(source).ok();
    }
    None
}

fn has_interactive_role(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "jsx_attribute"
            && jsx::jsx_attribute_name(child, source) == Some("role")
            && let Some(val) = jsx::jsx_attribute_value(child)
            && let Ok(text) = val.utf8_text(source)
        {
            let role = text.trim_matches(|c| c == '"' || c == '\'');
            if INTERACTIVE_ROLES.contains(&role) {
                return true;
            }
        }
    }
    false
}

fn has_tabindex(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let name = jsx::jsx_attribute_name(child, source);
        if name != Some("tabIndex") && name != Some("tabindex") {
            continue;
        }
        let Some(val) = jsx::jsx_attribute_value(child) else {
            return false;
        };
        let Ok(text) = val.utf8_text(source) else {
            return false;
        };
        let cleaned = text.trim_matches(|c| c == '"' || c == '\'' || c == '{' || c == '}');
        return cleaned != "-1";
    }
    false
}

fn is_interactive_element(node: tree_sitter::Node, source: &[u8]) -> bool {
    let elem = if node.kind() == "jsx_element" {
        node.child_by_field_name("open_tag")
    } else if node.kind() == "jsx_self_closing_element" {
        Some(node)
    } else {
        None
    };

    let Some(opening) = elem else { return false };

    if let Some(name) = get_element_name(opening, source) {
        let lower = name.to_lowercase();
        if INTERACTIVE_ELEMENTS.contains(&lower.as_str()) {
            return true;
        }
    }

    has_interactive_role(opening, source) || has_tabindex(opening, source)
}

fn find_nested_interactive<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if (child.kind() == "jsx_element" || child.kind() == "jsx_self_closing_element")
            && is_interactive_element(child, source)
        {
            return Some(child);
        }
        if let Some(found) = find_nested_interactive(child, source) {
            return Some(found);
        }
    }
    None
}

crate::ast_check! { on ["jsx_element"] => |node, source, ctx, diagnostics|
    if !is_interactive_element(node, source) {
        return;
    }

    // Search for nested interactive in the body
    if let Some(nested) = find_nested_interactive(node, source) {
        // Make sure nested is actually inside node's children, not the opening tag
        let node_start = node.start_byte();
        let node_end = node.end_byte();
        let nested_start = nested.start_byte();

        // Check that nested is a descendant (not the same node)
        if nested.id() != node.id() && nested_start > node_start && nested_start < node_end {
            let pos = nested.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "html-no-nested-interactive".into(),
                message: "Interactive element is nested inside another interactive element.".into(),
                severity: Severity::Warning,
                span: None,
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
    fn flags_button_in_button() {
        let d = run(r#"const x = <button><button>nested</button></button>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_link_in_button() {
        let d = run(r#"const x = <button><a href="/">link</a></button>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_button_in_link() {
        let d = run(r#"const x = <a href="/"><button>btn</button></a>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_input_in_button() {
        let d = run(r#"const x = <button><input type="text" /></button>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_role_button_nested() {
        let d = run(r#"const x = <div role="button"><button>x</button></div>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_tabindex_nested() {
        let d = run(r#"const x = <div tabIndex={0}><button>x</button></div>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_button_alone() {
        assert!(run(r#"const x = <button>click me</button>;"#).is_empty());
    }

    #[test]
    fn allows_non_interactive_in_button() {
        assert!(run(r#"const x = <button><span>text</span></button>;"#).is_empty());
    }

    #[test]
    fn allows_tabindex_negative_one() {
        assert!(run(r#"const x = <div tabIndex="-1"><button>ok</button></div>;"#).is_empty());
    }

    #[test]
    fn allows_sibling_buttons() {
        assert!(run(r#"const x = <><button>a</button><button>b</button></>;"#).is_empty());
    }
}
