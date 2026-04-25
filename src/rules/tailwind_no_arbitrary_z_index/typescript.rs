//! tailwind-no-arbitrary-z-index — flag arbitrary numeric z-index values
//! such as `z-[100]` inside `className`/`class` attribute strings.
//!
//! Walks JSX `jsx_attribute` nodes (TS/TSX/JS) and Vue `attribute` nodes
//! (Vue SFC `<template>`). For each `class`/`className` attribute, scans
//! every whitespace-separated token for `z-[N…]` where the first character
//! after the `[` is an ASCII digit. Named arbitrary values such as
//! `z-[var(--modal)]` are left alone — they already route through a token.

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

/// True when the class string contains a `z-[N…]` token whose first inner
/// character is an ASCII digit.
fn has_arbitrary_numeric_z(s: &str) -> bool {
    for token in s.split_whitespace() {
        if let Some(rest) = token.strip_prefix("z-[")
            && rest.starts_with(|c: char| c.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let class_str = jsx_class_value(node, source)
        .or_else(|| vue_class_value(node, source));
    let Some(class_str) = class_str else { return; };
    if has_arbitrary_numeric_z(class_str) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Use a design token (e.g. `z-10`, `z-50`) instead of an arbitrary z-index value.".into(),
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
    fn flags_arbitrary_z() {
        assert_eq!(
            run(r#"const x = <div className="z-[100] relative" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_large_z() {
        assert_eq!(run(r#"const x = <div className="z-[9999]" />;"#).len(), 1);
    }

    #[test]
    fn allows_token_z() {
        assert!(run(r#"const x = <div className="z-10 relative" />;"#).is_empty());
    }

    #[test]
    fn allows_named_z() {
        assert!(run(r#"const x = <div className="z-modal" />;"#).is_empty());
    }
}
