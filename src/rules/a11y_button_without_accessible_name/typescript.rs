//! Walk `jsx_element` nodes for `<button>...</button>`. If all children
//! are SVG / icon-component elements (or jsx_expression containing only
//! identifiers) and the opening tag has no `aria-label`/`aria-labelledby`/
//! `title`, flag the button.
//!
//! "Icon component" heuristic: a `jsx_self_closing_element` whose tag is
//! `svg` or starts with an uppercase letter and ends in `Icon`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_element_tag_name};

fn has_label_attr(opening: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = opening.walk();
    for child in opening.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(name) = jsx_attribute_name(child, source) else {
            continue;
        };
        if matches!(name, "aria-label" | "aria-labelledby" | "title") {
            return true;
        }
    }
    false
}

fn is_icon_tag(tag: &str) -> bool {
    tag == "svg" || tag.ends_with("Icon") || tag.ends_with("Svg")
}

fn child_provides_text(child: tree_sitter::Node, source: &[u8]) -> bool {
    match child.kind() {
        "jsx_text" => child
            .utf8_text(source)
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false),
        "jsx_self_closing_element" | "jsx_opening_element" => {
            let tag = jsx_element_tag_name(child, source);
            tag.is_some_and(|t| !is_icon_tag(t))
        }
        "jsx_element" => {
            // Find opening element to inspect tag.
            let Some(opening) = child.child(0) else {
                return true;
            };
            let tag = jsx_element_tag_name(opening, source);
            if tag.is_some_and(is_icon_tag) {
                // Scan inner children for text.
                let mut cursor = child.walk();
                for c in child.children(&mut cursor) {
                    if matches!(c.kind(), "jsx_text")
                        && c.utf8_text(source)
                            .map(|s| !s.trim().is_empty())
                            .unwrap_or(false)
                    {
                        return true;
                    }
                }
                false
            } else {
                true
            }
        }
        // Expression — assume it might render text; we don't flag.
        "jsx_expression" => true,
        _ => false,
    }
}

crate::ast_check! { on ["jsx_element"] =>
    |node, source, ctx, diagnostics|
    let Some(opening) = node.child(0) else { return; };
    if opening.kind() != "jsx_opening_element" { return; }
    let Some(tag) = jsx_element_tag_name(opening, source) else { return; };
    if tag != "button" { return; }
    if has_label_attr(opening, source) { return; }

    let mut cursor = node.walk();
    let mut any_text_child = false;
    for child in node.children(&mut cursor) {
        // Skip the opening / closing element nodes.
        if matches!(child.kind(), "jsx_opening_element" | "jsx_closing_element") {
            continue;
        }
        if child_provides_text(child, source) {
            any_text_child = true;
            break;
        }
    }

    if !any_text_child {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Icon-only `<button>` has no accessible name — add `aria-label` or visible text.".into(),
            Severity::Error,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_button_with_only_svg() {
        assert_eq!(run(r#"const x = <button><svg /></button>;"#).len(), 1);
    }

    #[test]
    fn flags_button_with_icon_component() {
        assert_eq!(run(r#"const x = <button><CloseIcon /></button>;"#).len(), 1);
    }

    #[test]
    fn allows_button_with_text() {
        assert!(run(r#"const x = <button>Save</button>;"#).is_empty());
    }

    #[test]
    fn allows_button_with_aria_label() {
        assert!(run(r#"const x = <button aria-label="Close"><CloseIcon /></button>;"#).is_empty());
    }

    #[test]
    fn allows_icon_button_with_visible_text() {
        assert!(run(r#"const x = <button><CloseIcon />Close</button>;"#).is_empty());
    }
}
