//! tailwind-no-conflicting-classes — flag mutually exclusive Tailwind
//! utility classes (e.g. `p-4 p-6`).
//!
//! Walks JSX `jsx_attribute` nodes (TS/TSX/JS) and Vue `attribute` nodes
//! (Vue SFC `<template>`). Groups class tokens by their conflict prefix
//! (`p-`, `px-`, `bg-`, …) or by membership in the `display` group; if a
//! group has 2+ entries, it reports the conflict.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};

/// Prefixes whose values are mutually exclusive. Each string is matched
/// as a prefix of the class name (e.g. "p-" matches "p-4", "p-8").
const CONFLICT_PREFIXES: &[&str] = &[
    // spacing
    "p-",
    "px-",
    "py-",
    "pt-",
    "pr-",
    "pb-",
    "pl-",
    "m-",
    "mx-",
    "my-",
    "mt-",
    "mr-",
    "mb-",
    "ml-",
    // sizing
    "w-",
    "h-",
    "min-w-",
    "min-h-",
    "max-w-",
    "max-h-",
    // typography (size)
    "text-",
    "font-",
    // backgrounds / borders / visual
    "bg-",
    "border-",
    "rounded-",
    "shadow-",
    "opacity-",
    "z-",
    // layout
    "gap-",
    "grid-cols-",
    "grid-rows-",
    "flex-",
    "justify-",
    "items-",
    "self-",
    "order-",
    "overflow-",
];

/// Display classes that conflict (only one can be active).
const DISPLAY_CLASSES: &[&str] = &[
    "block",
    "flex",
    "grid",
    "inline",
    "inline-block",
    "inline-flex",
    "inline-grid",
    "hidden",
    "table",
    "contents",
    "flow-root",
];

fn conflict_key(class: &str) -> Option<&'static str> {
    // Check longest prefix first to avoid "p-" matching "px-" classes.
    let mut prefixes: Vec<&&str> = CONFLICT_PREFIXES.iter().collect();
    prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
    for prefix in prefixes {
        if class.starts_with(*prefix) {
            return Some(prefix);
        }
    }
    if DISPLAY_CLASSES.contains(&class) {
        return Some("display");
    }
    None
}

fn jsx_class_value<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "jsx_attribute" {
        return None;
    }
    let name = crate::rules::jsx::jsx_attribute_name(node, source)?;
    if name != "className" && name != "class" {
        return None;
    }
    crate::rules::jsx::jsx_attribute_string_value(node, source)
}

fn vue_class_value<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "attribute" {
        return None;
    }
    let mut cursor = node.walk();
    let mut name: Option<&'a str> = None;
    let mut value: Option<&'a str> = None;
    for child in node.children(&mut cursor) {
        match child.kind() {
            "attribute_name" => name = child.utf8_text(source).ok(),
            "quoted_attribute_value" => {
                let mut vc = child.walk();
                value = child
                    .children(&mut vc)
                    .find(|c| c.kind() == "attribute_value")
                    .and_then(|c| c.utf8_text(source).ok());
            }
            _ => {}
        }
    }
    if name? != "class" {
        return None;
    }
    value
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let class_str = jsx_class_value(node, source)
        .or_else(|| vue_class_value(node, source));
    let Some(class_str) = class_str else { return; };
    let classes: Vec<&str> = class_str.split_whitespace().collect();
    let mut groups: HashMap<&str, Vec<&str>> = HashMap::new();
    for class in &classes {
        if let Some(key) = conflict_key(class) {
            groups.entry(key).or_default().push(class);
        }
    }
    for (prefix, members) in &groups {
        if members.len() >= 2 {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!(
                    "Conflicting `{prefix}` classes: {} — keep only one.",
                    members.join(", "),
                ),
                Severity::Warning,
            ));
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
    fn flags_conflicting_padding() {
        let diags = run(r#"const x = <div className="p-4 p-6" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("p-"));
    }

    #[test]
    fn flags_conflicting_text_size() {
        let diags = run(r#"const x = <div className="text-sm text-lg" />;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_conflicting_bg() {
        let diags = run(r#"const x = <div className="bg-red-500 bg-blue-500" />;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_display_conflict() {
        let diags = run(r#"const x = <div className="flex hidden" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("display"));
    }

    #[test]
    fn allows_non_conflicting() {
        assert!(run(r#"const x = <div className="p-4 mt-2 text-lg" />;"#).is_empty());
    }
}
