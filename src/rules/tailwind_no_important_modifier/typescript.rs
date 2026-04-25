//! tailwind-no-important-modifier — flag the Tailwind `!` important
//! modifier inside `className`/`class` attribute strings.
//!
//! Walks JSX `jsx_attribute` nodes (TS/TSX/JS) and Vue `attribute` nodes
//! (Vue SFC `<template>`). For each `class`/`className` attribute, scans
//! the string value for a `!utility` token (a `!` immediately followed by
//! an ASCII lowercase letter) and reports the first occurrence.

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

/// True when `s` contains a `!` immediately followed by an ASCII lowercase
/// letter — the Tailwind important-modifier shape (`!text-red-500`).
fn has_important_class(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes
        .windows(2)
        .any(|w| w[0] == b'!' && w[1].is_ascii_lowercase())
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let class_str = jsx_class_value(node, source)
        .or_else(|| vue_class_value(node, source));
    let Some(class_str) = class_str else { return; };
    if has_important_class(class_str) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Avoid the Tailwind `!` important modifier — fix specificity instead.".into(),
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
    fn flags_important_class() {
        assert_eq!(
            run(r#"const x = <div className="!text-red-500 flex" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_important_in_middle() {
        assert_eq!(
            run(r#"const x = <div className="w-full !hidden" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_normal_classes() {
        assert!(run(r#"const x = <div className="text-red-500 flex" />;"#).is_empty());
    }

    #[test]
    fn allows_exclamation_outside_classname() {
        assert!(run(r#"const x = <input placeholder="!important note" />;"#).is_empty());
    }
}
