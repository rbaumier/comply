//! a11y-no-aria-hidden-on-focusable AST backend.
//!
//! Flags `aria-hidden="true"` on elements that are natively focusable
//! (`button`, `a`, `input`, `select`, `textarea`) or have `tabIndex`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::jsx_attribute_name;

const FOCUSABLE_TAGS: &[&str] = &["button", "a", "input", "select", "textarea"];

/// Check whether a `jsx_attribute` has `aria-hidden` with a truthy value.
fn is_aria_hidden_true(attr: tree_sitter::Node, source: &[u8]) -> bool {
    if jsx_attribute_name(attr, source) != Some("aria-hidden") {
        return false;
    }
    // aria-hidden={true} or aria-hidden="true"
    let Some(value_node) = attr.child_by_field_name("value") else {
        // Bare `aria-hidden` without a value — treated as true in JSX.
        return true;
    };
    match value_node.kind() {
        "string" => {
            let Ok(text) = value_node.utf8_text(source) else {
                return false;
            };
            text.trim_matches(|c| c == '"' || c == '\'') == "true"
        }
        "jsx_expression" => {
            let Ok(text) = value_node.utf8_text(source) else {
                return false;
            };
            text.trim_matches(|c| c == '{' || c == '}').trim() == "true"
        }
        _ => false,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "jsx_opening_element" && kind != "jsx_self_closing_element" {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let Ok(tag) = name_node.utf8_text(source) else {
        return;
    };

    let mut has_aria_hidden = false;
    let mut has_tabindex = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if is_aria_hidden_true(child, source) {
            has_aria_hidden = true;
        }
        if matches!(jsx_attribute_name(child, source), Some("tabIndex" | "tabindex")) {
            has_tabindex = true;
        }
    }

    if !has_aria_hidden {
        return;
    }

    let is_focusable = FOCUSABLE_TAGS.contains(&tag) || has_tabindex;
    if is_focusable {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-no-aria-hidden-on-focusable".into(),
            message: "`aria-hidden=\"true\"` must not be set on focusable elements.".into(),
            severity: Severity::Error,
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
    fn flags_button_with_aria_hidden() {
        assert_eq!(run(r#"const x = <button aria-hidden="true">Click</button>;"#).len(), 1);
    }

    #[test]
    fn flags_aria_hidden_with_jsx_expression() {
        assert_eq!(run(r#"const x = <button aria-hidden={true}>Click</button>;"#).len(), 1);
    }

    #[test]
    fn flags_input_with_aria_hidden() {
        assert_eq!(run(r#"const x = <input aria-hidden="true" />;"#).len(), 1);
    }

    #[test]
    fn allows_div_with_aria_hidden() {
        assert!(run(r#"const x = <div aria-hidden="true">Hidden</div>;"#).is_empty());
    }

    #[test]
    fn flags_tabindex_with_aria_hidden() {
        assert_eq!(run(r#"const x = <div tabIndex={0} aria-hidden="true">Hidden</div>;"#).len(), 1);
    }
}
