//! html-require-input-label backend — flag inputs without accessible labels.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx;
use std::collections::HashSet;

const EXEMPT_INPUT_TYPES: &[&str] = &["hidden", "submit", "button", "reset", "image"];

fn get_jsx_element_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let opening = if node.kind() == "jsx_element" {
        node.child_by_field_name("open_tag")?
    } else if node.kind() == "jsx_self_closing_element" {
        node
    } else {
        return None;
    };

    let name_node = opening.child_by_field_name("name")?;
    name_node.utf8_text(source).ok()
}

fn get_opening_element(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    if node.kind() == "jsx_element" {
        node.child_by_field_name("open_tag")
    } else if node.kind() == "jsx_self_closing_element" {
        Some(node)
    } else {
        None
    }
}

fn get_attribute_value<'a>(
    opening: tree_sitter::Node<'a>,
    attr_name: &str,
    source: &'a [u8],
) -> Option<String> {
    let mut cursor = opening.walk();
    for child in opening.children(&mut cursor) {
        if child.kind() == "jsx_attribute"
            && jsx::jsx_attribute_name(child, source) == Some(attr_name)
            && let Some(val) = jsx::jsx_attribute_value(child)
            && let Ok(text) = val.utf8_text(source)
        {
            return Some(
                text.trim_matches(|c| c == '"' || c == '\'' || c == '{' || c == '}')
                    .to_string(),
            );
        }
    }
    None
}

fn has_aria_label(opening: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = opening.walk();
    for child in opening.children(&mut cursor) {
        if child.kind() == "jsx_attribute" {
            let name = jsx::jsx_attribute_name(child, source);
            if name == Some("aria-label") || name == Some("aria-labelledby") {
                return true;
            }
        }
    }
    false
}

fn is_exempt_input(opening: tree_sitter::Node, source: &[u8]) -> bool {
    if let Some(type_val) = get_attribute_value(opening, "type", source) {
        return EXEMPT_INPUT_TYPES.contains(&type_val.to_lowercase().as_str());
    }
    false
}

fn has_spread_attribute(opening: tree_sitter::Node) -> bool {
    let mut cursor = opening.walk();
    for child in opening.children(&mut cursor) {
        if child.kind() == "jsx_expression" {
            let mut inner = child.walk();
            for inner_child in child.children(&mut inner) {
                if inner_child.kind() == "spread_element" {
                    return true;
                }
            }
        }
    }
    false
}

fn is_inside_label(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "jsx_element"
            && let Some(name) = get_jsx_element_name(parent, source)
            && name.to_lowercase() == "label"
        {
            return true;
        }
        current = parent.parent();
    }
    false
}

struct LabelCollector {
    label_fors: HashSet<String>,
}

impl LabelCollector {
    fn new() -> Self {
        Self {
            label_fors: HashSet::new(),
        }
    }

    fn collect(&mut self, node: tree_sitter::Node, source: &[u8]) {
        if (node.kind() == "jsx_element" || node.kind() == "jsx_self_closing_element")
            && let Some(name) = get_jsx_element_name(node, source)
            && name.to_lowercase() == "label"
            && let Some(opening) = get_opening_element(node)
        {
            if let Some(for_val) = get_attribute_value(opening, "htmlFor", source) {
                self.label_fors.insert(for_val);
            }
            if let Some(for_val) = get_attribute_value(opening, "for", source) {
                self.label_fors.insert(for_val);
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect(child, source);
        }
    }
}

struct InputCollector<'a> {
    inputs: Vec<tree_sitter::Node<'a>>,
}

impl<'a> InputCollector<'a> {
    fn new() -> Self {
        Self { inputs: Vec::new() }
    }

    fn collect(&mut self, node: tree_sitter::Node<'a>, source: &'a [u8]) {
        if (node.kind() == "jsx_element" || node.kind() == "jsx_self_closing_element")
            && let Some(name) = get_jsx_element_name(node, source)
        {
            let lower = name.to_lowercase();
            if lower == "input" || lower == "select" || lower == "textarea" {
                self.inputs.push(node);
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect(child, source);
        }
    }
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let mut label_collector = LabelCollector::new();
    label_collector.collect(node, source);

    let mut input_collector = InputCollector::new();
    input_collector.collect(node, source);

    for input in input_collector.inputs {
        let Some(opening) = get_opening_element(input) else {
            continue;
        };

        // Skip exempt types
        if is_exempt_input(opening, source) {
            continue;
        }

        // Skip primitive components that spread props — callers supply labels via the spread
        if has_spread_attribute(opening) {
            continue;
        }

        // Check for aria-label/aria-labelledby
        if has_aria_label(opening, source) {
            continue;
        }

        // Check if wrapped in label
        if is_inside_label(input, source) {
            continue;
        }

        // Check if has id matching a label's htmlFor
        if let Some(id) = get_attribute_value(opening, "id", source)
            && label_collector.label_fors.contains(&id)
        {
            continue;
        }

        let pos = input.start_position();
        let name = get_jsx_element_name(input, source).unwrap_or("input");
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "html-require-input-label".into(),
            message: format!("<{name}> element must have an accessible label."),
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
    fn flags_input_without_label() {
        let d = run(r#"const x = <input type="text" />;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_select_without_label() {
        let d = run(r#"const x = <select><option>A</option></select>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_textarea_without_label() {
        let d = run(r#"const x = <textarea></textarea>;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_input_with_label_for() {
        let src = r#"const x = <><label htmlFor="name">Name</label><input id="name" /></>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_input_wrapped_in_label() {
        let src = r#"const x = <label>Name <input type="text" /></label>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_input_with_aria_label() {
        let src = r#"const x = <input aria-label="Name" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_input_with_aria_labelledby() {
        let src = r#"const x = <input aria-labelledby="name-label" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_hidden_input() {
        let src = r#"const x = <input type="hidden" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_submit_button() {
        let src = r#"const x = <input type="submit" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_button_input() {
        let src = r#"const x = <input type="button" />;"#;
        assert!(run(src).is_empty());
    }

    // Regression #485: base UI primitive spreading restProps — caller provides label
    #[test]
    fn no_fp_on_input_with_spread_props() {
        let src = r#"const x = <input className="x" data-slot="input" {...restProps} />;"#;
        assert!(run(src).is_empty());
    }
}
