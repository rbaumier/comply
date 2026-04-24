//! Flags `<Image source="url">` (string-literal source attribute).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let tag_node = match node.kind() {
        "jsx_self_closing_element" => node,
        "jsx_opening_element" => node,
        _ => return,
    };
    let Some(name_node) = tag_node.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };
    if tag != "Image" { return; }

    let mut cursor = tag_node.walk();
    for child in tag_node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(child, source) else { continue };
        if attr_name != "source" { continue; }
        let Some(value) = crate::rules::jsx::jsx_attribute_value(child) else { continue };
        if value.kind() == "string" {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &value,
                super::META.id,
                "`<Image source=\"...\">` with a string literal renders nothing — use `{{ uri: '...' }}` or `require(...)`.".into(),
                Severity::Warning,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_string_source() {
        let src = "const x = <Image source=\"https://a.b/c.png\" />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_uri_object() {
        let src = "const x = <Image source={{ uri: 'https://a.b/c.png' }} />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_require() {
        let src = "const x = <Image source={require('./img.png')} />;";
        assert!(run(src).is_empty());
    }
}
