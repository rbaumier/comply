//! a11y-alt-text backend — AST-based detection.
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };

    let needs_alt = match tag {
        "img" | "area" => true,
        "input" => {
            // Only <input type="image"> needs alt
            let mut cursor = node.walk();
            let mut is_image = false;
            for child in node.children(&mut cursor) {
                if child.kind() != "jsx_attribute" { continue; }
                if crate::rules::jsx::jsx_attribute_name(child, source) != Some("type") { continue; }
                if let Some(val) = crate::rules::jsx::jsx_attribute_string_value(child, source)
                    && val == "image"
                {
                    is_image = true;
                }
            }
            is_image
        }
        _ => false,
    };

    if !needs_alt { return; }

    // Check if alt= attribute exists
    let mut cursor = node.walk();
    let has_alt = node.children(&mut cursor).any(|child| {
        crate::rules::jsx::jsx_attribute_name(child, source) == Some("alt")
    });

    if !has_alt {
        let pos = node.start_position();
        let msg = match tag {
            "img" => "`<img>` is missing an `alt` attribute.",
            "area" => "`<area>` is missing an `alt` attribute.",
            _ => "`<input type=\"image\">` is missing an `alt` attribute.",
        };
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "a11y-alt-text".into(),
            message: msg.into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_img_without_alt() {
        assert_eq!(run_on("const x = <img src=\"logo.png\" />;").len(), 1);
    }

    #[test]
    fn allows_img_with_alt() {
        assert!(run_on("const x = <img alt=\"Logo\" src=\"logo.png\" />;").is_empty());
    }

    #[test]
    fn flags_area_without_alt() {
        assert_eq!(run_on("const x = <area shape=\"rect\" />;").len(), 1);
    }

    #[test]
    fn flags_input_type_image_without_alt() {
        assert_eq!(
            run_on("const x = <input type=\"image\" src=\"btn.png\" />;").len(),
            1
        );
    }

    #[test]
    fn allows_input_type_image_with_alt() {
        assert!(
            run_on("const x = <input type=\"image\" alt=\"Submit\" src=\"btn.png\" />;").is_empty()
        );
    }

    #[test]
    fn allows_regular_input() {
        assert!(run_on("const x = <input type=\"text\" />;").is_empty());
    }
}
