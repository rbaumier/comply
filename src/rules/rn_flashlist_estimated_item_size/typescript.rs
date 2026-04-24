//! Flags `<FlashList>` elements that lack an `estimatedItemSize` attribute.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let tag_node = match node.kind() {
        "jsx_self_closing_element" => node,
        "jsx_opening_element" => node,
        _ => return,
    };
    let Some(name_node) = tag_node.child_by_field_name("name") else { return };
    let Ok(tag) = name_node.utf8_text(source) else { return };
    if tag != "FlashList" { return; }

    let mut cursor = tag_node.walk();
    let has_attr = tag_node.children(&mut cursor).any(|child| {
        if child.kind() != "jsx_attribute" { return false; }
        let Some(attr_name) = crate::rules::jsx::jsx_attribute_name(child, source) else { return false };
        attr_name == "estimatedItemSize"
    });
    if has_attr { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &tag_node,
        super::META.id,
        "`<FlashList>` is missing `estimatedItemSize` — required for performance.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_missing_estimated() {
        let src = "const x = <FlashList data={items} renderItem={r} />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_estimated() {
        let src = "const x = <FlashList data={items} renderItem={r} estimatedItemSize={64} />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_flatlist() {
        let src = "const x = <FlatList data={items} renderItem={r} />;";
        assert!(run(src).is_empty());
    }
}
