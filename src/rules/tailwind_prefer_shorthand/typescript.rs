//! tailwind-prefer-shorthand — flag Tailwind utility pairs that share the
//! same value and can be collapsed into a shorter shorthand utility.
//!
//! Walks JSX `jsx_attribute` nodes (TS/TSX/JS) and Vue `attribute` nodes
//! (Vue SFC `<template>`). For each `class`/`className` attribute, splits
//! the value on whitespace and bins each token by its variant prefix
//! (`md:`, `hover:`) and `!` important modifier; for each bin, checks the
//! `SHORTHAND_PAIRS` table for matching `(left, right)` prefixes whose
//! values are identical and reports the collapsible pair.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};

/// Pair of prefixes that can collapse into one shorthand when their value matches.
const SHORTHAND_PAIRS: &[(&str, &str, &str)] = &[
    // padding
    ("px-", "py-", "p-"),
    ("pt-", "pb-", "py-"),
    ("pl-", "pr-", "px-"),
    // margin
    ("mx-", "my-", "m-"),
    ("mt-", "mb-", "my-"),
    ("ml-", "mr-", "mx-"),
    // inset
    ("top-", "bottom-", "inset-y-"),
    ("left-", "right-", "inset-x-"),
    // scroll padding
    ("scroll-px-", "scroll-py-", "scroll-p-"),
    ("scroll-pt-", "scroll-pb-", "scroll-py-"),
    ("scroll-pl-", "scroll-pr-", "scroll-px-"),
    // scroll margin
    ("scroll-mx-", "scroll-my-", "scroll-m-"),
    ("scroll-mt-", "scroll-mb-", "scroll-my-"),
    ("scroll-ml-", "scroll-mr-", "scroll-mx-"),
    // border radius corners
    ("rounded-t-", "rounded-b-", "rounded-y-"),
    ("rounded-l-", "rounded-r-", "rounded-x-"),
    // sizing
    ("w-", "h-", "size-"),
];

fn split_variant(class: &str) -> (&str, &str) {
    match class.rfind(':') {
        Some(idx) => (&class[..=idx], &class[idx + 1..]),
        None => ("", class),
    }
}

fn strip_important(class: &str) -> (bool, &str) {
    match class.strip_prefix('!') {
        Some(rest) => (true, rest),
        None => (false, class),
    }
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

    // Group classes by (variant, important) so we only pair classes with
    // the same variants (e.g. `md:pt-2` with `md:pb-2`).
    let mut buckets: HashMap<(&str, bool), Vec<&str>> = HashMap::new();
    for class in class_str.split_whitespace() {
        let (variant, rest) = split_variant(class);
        let (imp, base) = strip_important(rest);
        buckets.entry((variant, imp)).or_default().push(base);
    }

    for ((variant, important), bases) in buckets {
        for &(left_prefix, right_prefix, short_prefix) in SHORTHAND_PAIRS {
            let left_value = bases.iter().find_map(|b| b.strip_prefix(left_prefix));
            let right_value = bases.iter().find_map(|b| b.strip_prefix(right_prefix));
            if let (Some(lv), Some(rv)) = (left_value, right_value)
                && lv == rv
                && !lv.is_empty()
            {
                let bang = if important { "!" } else { "" };
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    format!(
                        "Prefer shorthand: `{variant}{bang}{left_prefix}{lv} {variant}{bang}{right_prefix}{rv}` can be written as `{variant}{bang}{short_prefix}{lv}`."
                    ),
                    Severity::Warning,
                ));
            }
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
    fn flags_px_py_same_value() {
        let diags = run(r#"const x = <div className="px-2 py-2" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("p-2"));
    }

    #[test]
    fn flags_pt_pb_same_value() {
        let diags = run(r#"const x = <div className="pt-4 pb-4" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("py-4"));
    }

    #[test]
    fn flags_ml_mr_same_value() {
        let diags = run(r#"const x = <div className="ml-1 mr-1" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("mx-1"));
    }

    #[test]
    fn flags_rounded_corners() {
        let diags = run(r#"const x = <div className="rounded-t-lg rounded-b-lg" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("rounded-y-lg"));
    }

    #[test]
    fn flags_with_same_variant() {
        let diags = run(r#"const x = <div className="md:px-2 md:py-2" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("md:p-2"));
    }

    #[test]
    fn allows_different_values() {
        assert!(run(r#"const x = <div className="px-2 py-4" />;"#).is_empty());
    }

    #[test]
    fn allows_different_variants() {
        assert!(run(r#"const x = <div className="md:px-2 py-2" />;"#).is_empty());
    }

    #[test]
    fn allows_standalone_axis() {
        assert!(run(r#"const x = <div className="px-2" />;"#).is_empty());
    }
}
