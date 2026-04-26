//! react-void-dom-elements-no-children AST backend.
//!
//! Flags void HTML elements (`<br>`, `<img>`, `<input>`, etc.) that have
//! children (text content, child elements, or `children`/`dangerouslySetInnerHTML` props).

use crate::diagnostic::{Diagnostic, Severity};

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen",
    "link", "meta", "param", "source", "track", "wbr",
];

fn is_void_element(name: &str) -> bool {
    VOID_ELEMENTS.contains(&name)
}

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] => |node, source, ctx, diagnostics|
    // Match both opening elements (with children) and self-closing elements (with bad props)
    let kind = node.kind();
    // Extract the element name
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(tag_name) = name_node.utf8_text(source) else { return };
    if !is_void_element(tag_name) {
        return;
    }

    // For opening elements: the parent jsx_element having children means
    // there's content between <tag> and </tag>
    if kind == "jsx_opening_element"
        && let Some(parent) = node.parent()
            && parent.kind() == "jsx_element" {
                // Check if parent has any children beyond the opening/closing elements
                let child_count = parent.named_child_count();
                // jsx_element children: opening_element, [content...], closing_element
                // If > 2 named children, there's content between tags
                if child_count > 2 {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "react-void-dom-elements-no-children".into(),
                        message: format!("`<{tag_name}>` is a void element and cannot have children."),
                        severity: Severity::Error,
                        span: None,
                    });
                    return;
                }
                // Even if child_count == 2, the element has open+close tags which
                // means it's non-self-closing — that alone is suspicious for void
                // elements, but we only flag when there's actual content or bad props.
            }

    // Check for `children` or `dangerouslySetInnerHTML` props
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name_node) = child.child(0) else { continue };
        let Ok(attr_name) = attr_name_node.utf8_text(source) else { continue };
        if attr_name == "children" || attr_name == "dangerouslySetInnerHTML" {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "react-void-dom-elements-no-children".into(),
                message: format!("`<{tag_name}>` is a void element and cannot have children."),
                severity: Severity::Error,
                span: None,
            });
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_br_with_children() {
        let src = "const x = <br>text</br>;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_img_with_children_prop() {
        let src = r#"const x = <img children={<span />} />;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_hr_with_danger() {
        let src = r#"const x = <hr dangerouslySetInnerHTML={{ __html: "x" }} />;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_self_closing_void() {
        let src = r#"const x = <br />;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_div_with_children() {
        let src = "const x = <div>text</div>;";
        assert!(run_on(src).is_empty());
    }
}
