//! tailwind-prefer-size-shorthand — flag `w-X h-X` pairs with matching
//! values so the caller can collapse them to the Tailwind v3.4+ `size-X`
//! shorthand.
//!
//! Walks JSX `jsx_attribute` nodes (TS/TSX/JS) and Vue `attribute` nodes
//! (Vue SFC `<template>`). For each `class`/`className` attribute, looks
//! for a `w-V` token whose value `V` also appears in some `h-V` token in
//! the same string.

use crate::diagnostic::{Diagnostic, Severity};

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

/// Return the matching `w-V`/`h-V` value if both appear in the class string.
fn find_wh_duplicate(class_str: &str) -> Option<&str> {
    let tokens: Vec<&str> = class_str.split_whitespace().collect();
    let w_vals: Vec<&str> = tokens.iter().filter_map(|t| t.strip_prefix("w-")).collect();
    let h_vals: Vec<&str> = tokens.iter().filter_map(|t| t.strip_prefix("h-")).collect();
    w_vals.into_iter().find(|w| h_vals.contains(w))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let class_str = jsx_class_value(node, source)
        .or_else(|| vue_class_value(node, source));
    let Some(class_str) = class_str else { return; };
    if let Some(val) = find_wh_duplicate(class_str) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!("Replace `w-{val} h-{val}` with `size-{val}`."),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(src, &Check)
    }

    #[test]
    fn flags_equal_w_h() {
        assert_eq!(
            run(r#"const x = <div className="w-4 h-4 flex" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_full() {
        assert_eq!(
            run(r#"const x = <div className="w-full h-full" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_different_values() {
        assert!(run(r#"const x = <div className="w-4 h-6" />;"#).is_empty());
    }

    #[test]
    fn allows_size_shorthand_already() {
        assert!(run(r#"const x = <div className="size-4" />;"#).is_empty());
    }
}
