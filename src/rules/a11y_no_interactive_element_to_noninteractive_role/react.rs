//! a11y-no-interactive-element-to-noninteractive-role AST backend.

use crate::diagnostic::{Diagnostic, Severity};

const INTERACTIVE_ELEMENTS: &[&str] = &["button", "a", "input", "select", "textarea"];

const NON_INTERACTIVE_ROLES: &[&str] = &[
    "article", "banner", "complementary", "contentinfo", "document",
    "img", "list", "listitem", "note", "presentation", "none", "heading",
];

/// Extract the string value from a JSX attribute value node.
fn attr_string_value<'a>(attr: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    crate::rules::jsx::jsx_attribute_string_value(attr, source)
}

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };

    if !INTERACTIVE_ELEMENTS.contains(&tag) {
        return;
    }

    // Look for role attribute
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
            && NON_INTERACTIVE_ROLES.contains(&role) {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "a11y-no-interactive-element-to-noninteractive-role".into(),
                    message: format!(
                        "Interactive element should not have non-interactive `role=\"{role}\"`."
                    ),
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
    fn flags_button_with_article_role() {
        assert_eq!(run(r#"const x = <button role="article">X</button>;"#).len(), 1);
    }

    #[test]
    fn flags_a_with_presentation_role() {
        assert_eq!(run(r##"const x = <a href="#" role="presentation">link</a>;"##).len(), 1);
    }

    #[test]
    fn allows_button_with_interactive_role() {
        assert!(run(r#"const x = <button role="menuitem">X</button>;"#).is_empty());
    }

    #[test]
    fn allows_div_with_noninteractive_role() {
        assert!(run(r#"const x = <div role="article">X</div>;"#).is_empty());
    }
}
