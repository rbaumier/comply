//! a11y-control-has-associated-label backend — AST-based detection.
use crate::diagnostic::{Diagnostic, Severity};

const INTERACTIVE_ELEMENTS: &[&str] = &["button", "input", "select", "textarea"];

crate::ast_check! { |node, source, ctx, diagnostics|
    let is_self_closing = node.kind() == "jsx_self_closing_element";
    let is_element = node.kind() == "jsx_element";

    if !is_self_closing && !is_element {
        return;
    }

    // For self-closing, check tag directly; for element, check opening tag.
    let tag_node = if is_self_closing {
        node
    } else {
        let Some(opening) = node.child(0) else { return };
        if opening.kind() != "jsx_opening_element" { return; }
        opening
    };

    let Some(name_node) = tag_node.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };

    if !INTERACTIVE_ELEMENTS.contains(&tag) { return; }

    // <input type="hidden"> is exempt
    if tag == "input" {
        let mut cursor = tag_node.walk();
        for child in tag_node.children(&mut cursor) {
            if crate::rules::jsx::jsx_attribute_name(child, source) != Some("type") { continue; }
            if let Some(val) = crate::rules::jsx::jsx_attribute_string_value(child, source)
                && val == "hidden"
            {
                return;
            }
        }
    }

    // Check for aria-label or aria-labelledby
    let mut cursor2 = tag_node.walk();
    let has_label_attr = tag_node.children(&mut cursor2).any(|child| {
        let name = crate::rules::jsx::jsx_attribute_name(child, source);
        name == Some("aria-label") || name == Some("aria-labelledby")
    });
    if has_label_attr { return; }

    // For <button> elements that are not self-closing, check for text content
    if tag == "button" && is_element {
        let mut el_cursor = node.walk();
        let has_content = node.children(&mut el_cursor).any(|child| {
            match child.kind() {
                "jsx_text" => {
                    let Ok(text) = child.utf8_text(source) else { return false };
                    !text.trim().is_empty()
                }
                "jsx_element" | "jsx_self_closing_element" | "jsx_expression" => true,
                _ => false,
            }
        });
        if has_content { return; }
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "a11y-control-has-associated-label".into(),
        message: "Interactive element is missing an accessible label (`aria-label` or `aria-labelledby`).".into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_button_without_label() {
        assert_eq!(run_on(r#"const x = <button />;"#).len(), 1);
    }

    #[test]
    fn flags_input_without_label() {
        assert_eq!(run_on(r#"const x = <input />;"#).len(), 1);
    }

    #[test]
    fn allows_input_with_aria_label() {
        assert!(run_on(r#"const x = <input aria-label="Name" />;"#).is_empty());
    }

    #[test]
    fn allows_hidden_input() {
        assert!(run_on(r#"const x = <input type="hidden" />;"#).is_empty());
    }

    #[test]
    fn allows_button_with_text_content() {
        assert!(run_on(r#"const x = <button>Submit</button>;"#).is_empty());
    }
}
