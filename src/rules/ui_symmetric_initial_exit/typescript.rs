//! Compare property keys of `initial` and `exit` object literals on a
//! `motion.*` component. Flag when the key sets differ.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::BTreeSet;

crate::ast_check! { on ["jsx_opening_element", "jsx_self_closing_element"] prefilter = ["motion."] => |node, source, ctx, diagnostics|
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else { return };
    if !tag.starts_with("motion.") { return; }

    let mut initial_keys: Option<BTreeSet<String>> = None;
    let mut exit_keys: Option<BTreeSet<String>> = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" { continue; }
        let Some(name) = crate::rules::jsx::jsx_attribute_name(child, source) else { continue };
        if name != "initial" && name != "exit" { continue; }
        let keys = extract_object_keys(child, source);
        if keys.is_empty() { continue; }
        if name == "initial" { initial_keys = Some(keys); }
        else { exit_keys = Some(keys); }
    }

    let (Some(init), Some(ex)) = (initial_keys, exit_keys) else { return };
    if init == ex { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "<{tag}> `initial` keys {init:?} don't match `exit` keys {ex:?} — enter and exit won't mirror."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

/// Walk the attribute's value expression, find the first `object` node,
/// and collect its `pair` keys.
fn extract_object_keys(attr: tree_sitter::Node, source: &[u8]) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let Some(value) = crate::rules::jsx::jsx_attribute_value(attr) else {
        return keys;
    };
    let object = find_object(value);
    let Some(obj) = object else {
        return keys;
    };
    let mut cursor = obj.walk();
    for child in obj.children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        if let Some(key) = child.child_by_field_name("key")
            && let Ok(text) = key.utf8_text(source)
        {
            keys.insert(text.trim_matches(|c| c == '"' || c == '\'').to_string());
        }
    }
    keys
}

fn find_object(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    if node.kind() == "object" {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_object(child) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_mismatched_keys() {
        let src = r#"
            const x = <motion.div
                initial={{ opacity: 0, y: 10 }}
                exit={{ opacity: 0 }}
            />;
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_matching_keys() {
        let src = r#"
            const x = <motion.div
                initial={{ opacity: 0, y: 10 }}
                exit={{ opacity: 0, y: 10 }}
            />;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_without_exit() {
        let src = r#"
            const x = <motion.div initial={{ opacity: 0 }} />;
        "#;
        assert!(run(src).is_empty());
    }
}
