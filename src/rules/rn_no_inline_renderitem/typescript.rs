//! Flags `renderItem={(...) => ...}` / `renderItem={function(...) {...}}` on JSX elements.

use crate::diagnostic::{Diagnostic, Severity};

const RN_LIST_COMPONENTS: &[&str] = &[
    "FlatList", "SectionList", "FlashList", "VirtualizedList", "SwipeListView",
];

fn is_rn_list_component(node: tree_sitter::Node, source: &[u8]) -> bool {
    let parent = node.parent();
    let element = match parent.map(|p| p.kind()) {
        Some("jsx_self_closing_element") | Some("jsx_opening_element") => parent.unwrap(),
        _ => return false,
    };
    let mut cursor = element.walk();
    for child in element.children(&mut cursor) {
        if child.kind() == "identifier" || child.kind() == "member_expression" {
            let tag = child.utf8_text(source).unwrap_or("");
            return RN_LIST_COMPONENTS.iter().any(|c| tag.ends_with(c));
        }
    }
    false
}

crate::ast_check! { on ["jsx_attribute"] prefilter = ["renderItem"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    if name != "renderItem" { return; }
    if !is_rn_list_component(node, source) { return; }
    let Some(value) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    if value.kind() != "jsx_expression" { return; }
    // Find the inner expression.
    let mut cursor = value.walk();
    for child in value.children(&mut cursor) {
        match child.kind() {
            "{" | "}" => continue,
            "arrow_function" | "function_expression" | "function" => {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &child,
                    super::META.id,
                    "Inline function in `renderItem` creates a new reference every render — extract to a stable component or `useCallback`.".into(),
                    Severity::Warning,
                ));
                return;
            }
            _ => return,
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
    fn flags_inline_arrow() {
        let src = "const x = <FlatList renderItem={({ item }) => <Row item={item} />} />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_function_expression() {
        let src = "const x = <FlatList renderItem={function ({ item }) { return null; }} />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_extracted_handler() {
        let src = "const x = <FlatList renderItem={renderRow} />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_inline_arrow_flashlist() {
        let src = "const x = <FlashList renderItem={({ item }) => <Row />} />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_custom_component() {
        let src = "const x = <CustomRenderer renderItem={() => <View />} />;";
        assert!(run(src).is_empty());
    }
}
