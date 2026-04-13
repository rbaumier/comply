//! a11y-img-redundant-alt AST backend.
//!
//! Flags `<img>` elements whose `alt` text contains redundant words
//! like "image", "picture", or "photo".

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::jsx_attribute_name;

fn has_redundant_word(alt: &str) -> bool {
    let lower = alt.to_ascii_lowercase();
    lower.contains("image") || lower.contains("picture") || lower.contains("photo")
}

/// Extract the string value from a `jsx_attribute` node (the child after `=`).
fn jsx_attr_string_value<'a>(attr: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    crate::rules::jsx::jsx_attribute_string_value(attr, source)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "jsx_self_closing_element" && kind != "jsx_opening_element" {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let Ok(tag) = name_node.utf8_text(source) else {
        return;
    };
    if tag != "img" {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if jsx_attribute_name(child, source) != Some("alt") {
            continue;
        }
        if let Some(val) = jsx_attr_string_value(child, source)
            && has_redundant_word(val) {
                let pos = child.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "a11y-img-redundant-alt".into(),
                    message: "`alt` text should not contain words like \"image\", \"picture\", or \"photo\" — describe the content instead.".into(),
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
    fn flags_alt_with_image() {
        assert_eq!(run(r#"const x = <img alt="An image of a cat" src="cat.png" />;"#).len(), 1);
    }

    #[test]
    fn flags_alt_with_photo_case_insensitive() {
        assert_eq!(run(r#"const x = <img alt="Photo of sunset" src="sunset.png" />;"#).len(), 1);
    }

    #[test]
    fn flags_alt_with_picture() {
        assert_eq!(run(r#"const x = <img alt="A picture" src="pic.png" />;"#).len(), 1);
    }

    #[test]
    fn allows_descriptive_alt() {
        assert!(run(r#"const x = <img alt="A golden retriever playing fetch" src="dog.png" />;"#).is_empty());
    }
}
