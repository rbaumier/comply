//! tailwind-no-deprecated-classes — flag deprecated Tailwind utility
//! classes that were removed or renamed in v3/v4.
//!
//! Walks JSX `jsx_attribute` nodes (TS/TSX/JS) and Vue `attribute` nodes
//! (Vue SFC `<template>`). For each `class`/`className` attribute, splits
//! the value on whitespace, strips Tailwind variant prefixes (`hover:`,
//! `md:`) and the `!` important modifier, and reports any token whose
//! base form matches the deprecation table.

use crate::diagnostic::{Diagnostic, Severity};

/// Deprecated class → recommended replacement.
const DEPRECATED: &[(&str, &str)] = &[
    ("flex-grow-0", "grow-0"),
    ("flex-grow", "grow"),
    ("flex-shrink-0", "shrink-0"),
    ("flex-shrink", "shrink"),
    ("overflow-ellipsis", "text-ellipsis"),
    ("overflow-clip", "text-clip"),
    ("decoration-slice", "box-decoration-slice"),
    ("decoration-clone", "box-decoration-clone"),
];

fn replacement_for(class: &str) -> Option<&'static str> {
    DEPRECATED
        .iter()
        .find(|(dep, _)| *dep == class)
        .map(|(_, repl)| *repl)
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
    for class in class_str.split_whitespace() {
        let base = class.rsplit(':').next().unwrap_or(class);
        let base = base.strip_prefix('!').unwrap_or(base);
        if let Some(replacement) = replacement_for(base) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!("Deprecated Tailwind class `{base}` — use `{replacement}` instead."),
                Severity::Warning,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(src, &Check)
    }

    #[test]
    fn flags_flex_grow_0() {
        let diags = run(r#"const x = <div className="flex-grow-0" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("grow-0"));
    }

    #[test]
    fn flags_overflow_ellipsis() {
        let diags = run(r#"const x = <div className="truncate overflow-ellipsis" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("text-ellipsis"));
    }

    #[test]
    fn flags_decoration_clone() {
        let diags = run(r#"const x = <div className="decoration-clone" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("box-decoration-clone"));
    }

    #[test]
    fn flags_with_variant() {
        let diags = run(r#"const x = <div className="hover:flex-shrink" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("shrink"));
    }

    #[test]
    fn allows_current_classes() {
        assert!(run(r#"const x = <div className="grow shrink p-4 text-ellipsis" />;"#).is_empty());
    }

    #[test]
    fn flags_in_class_attr() {
        let diags = run(r#"const x = <div class="flex-shrink-0" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("shrink-0"));
    }
}
