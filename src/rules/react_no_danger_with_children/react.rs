//! react-no-danger-with-children AST backend.
//!
//! Detects co-occurrence of `dangerouslySetInnerHTML` and `children`
//! (either as a prop or as text content) on the same JSX element.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_self_closing_element", "jsx_element"] => |node, source, ctx, diagnostics|
    let is_self_closing = node.kind() == "jsx_self_closing_element";
    let is_element = node.kind() == "jsx_element";

    let tag_node = if is_self_closing {
        node
    } else {
        let Some(opening) = node.child(0) else { return };
        if opening.kind() != "jsx_opening_element" { return; }
        opening
    };

    // Check attributes for dangerouslySetInnerHTML and children prop.
    let mut cursor = tag_node.walk();
    let mut has_danger = false;
    let mut has_children_prop = false;

    for child in tag_node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        let Some(attr_name) = child.child(0) else { continue };
        let Ok(name_text) = attr_name.utf8_text(source) else { continue };
        match name_text {
            "dangerouslySetInnerHTML" => has_danger = true,
            "children" => has_children_prop = true,
            _ => {}
        }
    }

    if !has_danger {
        return;
    }

    // For jsx_element, also check for text content children.
    let has_text_children = if is_element {
        let mut el_cursor = node.walk();
        node.children(&mut el_cursor).any(|child| {
            match child.kind() {
                "jsx_text" => {
                    let Ok(text) = child.utf8_text(source) else { return false };
                    !text.trim().is_empty()
                }
                "jsx_expression" | "jsx_element" | "jsx_self_closing_element" => true,
                _ => false,
            }
        })
    } else {
        false
    };

    if has_children_prop || has_text_children {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-no-danger-with-children".into(),
            message: "Using both `dangerouslySetInnerHTML` and \
                      `children` on the same element is invalid — \
                      React will throw at runtime."
                .into(),
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
    fn flags_danger_with_children_prop() {
        let src =
            r#"const x = <div dangerouslySetInnerHTML={{ __html: html }} children="text" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_danger_with_text_children() {
        let src = r#"const x = <div dangerouslySetInnerHTML={{ __html: html }}>Some text</div>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_danger_without_children() {
        let src = r#"const x = <div dangerouslySetInnerHTML={{ __html: html }} />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_children_without_danger() {
        let src = "const x = <div>Some text</div>;";
        assert!(run(src).is_empty());
    }
}
