//! a11y-role-has-required-aria-props AST backend.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns the required ARIA props for a given role.
fn required_props(role: &str) -> &'static [&'static str] {
    match role {
        "checkbox" | "radio" => &["aria-checked"],
        "slider" => &["aria-valuenow", "aria-valuemin", "aria-valuemax"],
        "combobox" => &["aria-expanded"],
        "scrollbar" => &["aria-controls", "aria-valuenow"],
        _ => &[],
    }
}

/// Extract the string value from a JSX attribute value node.
fn attr_string_value<'a>(attr: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    crate::rules::jsx::jsx_attribute_string_value(attr, source)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_opening_element" && node.kind() != "jsx_self_closing_element" {
        return;
    }

    // Collect all attributes
    let mut role_value: Option<String> = None;
    let mut present_attrs = Vec::new();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name) = attr_name.utf8_text(source) else { continue };
        present_attrs.push(name.to_string());
        if name == "role"
            && let Some(val) = attr_string_value(child, source) {
                role_value = Some(val.to_string());
            }
    }

    let Some(role) = role_value else { return };
    let props = required_props(&role);
    if props.is_empty() {
        return;
    }

    let missing: Vec<&str> = props
        .iter()
        .filter(|prop| !present_attrs.iter().any(|a| a == **prop))
        .copied()
        .collect();

    if !missing.is_empty() {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-role-has-required-aria-props".into(),
            message: format!(
                "`role=\"{}\"` is missing required ARIA props: {}.",
                role,
                missing.join(", ")
            ),
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
    fn flags_checkbox_missing_aria_checked() {
        assert_eq!(run(r#"const x = <div role="checkbox" />;"#).len(), 1);
    }

    #[test]
    fn allows_checkbox_with_aria_checked() {
        assert!(run(r#"const x = <div role="checkbox" aria-checked="false" />;"#).is_empty());
    }

    #[test]
    fn flags_slider_missing_props() {
        let diags = run(r#"const x = <div role="slider" aria-valuenow={5} />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("aria-valuemin"));
    }

    #[test]
    fn allows_slider_with_all_props() {
        let src = r#"const x = <div role="slider" aria-valuenow={5} aria-valuemin={0} aria-valuemax={10} />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_combobox_missing_expanded() {
        assert_eq!(run(r#"const x = <div role="combobox" />;"#).len(), 1);
    }
}
