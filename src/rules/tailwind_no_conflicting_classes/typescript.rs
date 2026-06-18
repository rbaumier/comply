//! tailwind-no-conflicting-classes — flag mutually exclusive Tailwind
//! utility classes (e.g. `p-4 p-6`).
//!
//! Walks JSX `jsx_attribute` nodes (TS/TSX/JS) and Vue `attribute` nodes
//! (Vue SFC `<template>`). Groups class tokens by their conflict prefix
//! (`p-`, `px-`, `bg-`, …) or by membership in the `display` group; if a
//! group has 2+ entries, it reports the conflict.

use rustc_hash::FxHashMap;

use crate::diagnostic::{Diagnostic, Severity};

/// Prefixes whose values are unambiguously mutually exclusive — any two
/// classes sharing one of these prefixes conflict. Ambiguous prefixes
/// (`text-`, `font-`, `flex-`, `border-`) are handled by dedicated
/// sub-categorisation functions that split by CSS property.
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
    // visual
    "bg-",
    "rounded-",
    "shadow-",
    "opacity-",
    "z-",
    // layout
    "gap-",
    "gap-x-",
    "gap-y-",
    "grid-cols-",
    "grid-rows-",
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

/// Detect CSS value type from an arbitrary Tailwind value (inside `[...]`).
/// Mirrors Tailwind's own type inference: lengths → size, colors → color.
fn css_value_type(value: &str) -> Option<&'static str> {
    let v = value.trim();
    if v.starts_with('#')
        || v.starts_with("rgb")
        || v.starts_with("hsl")
        || v.starts_with("oklch")
        || v.starts_with("hwb")
        || v.starts_with("lab")
        || v.starts_with("lch")
        || v.starts_with("color(")
    {
        return Some("color");
    }
    const LENGTH_UNITS: &[&str] = &[
        "px", "rem", "em", "%", "vw", "vh", "dvh", "svh", "lvh", "vmin", "vmax", "ch", "ex", "cap",
        "lh", "rlh", "pt", "pc", "mm", "cm", "in",
    ];
    if LENGTH_UNITS.iter().any(|u| v.ends_with(u))
        || v.starts_with("calc(")
        || v.starts_with("clamp(")
        || v.starts_with("min(")
        || v.starts_with("max(")
    {
        return Some("length");
    }
    if v.starts_with("var(--") || v.starts_with("--") {
        return None;
    }
    None
}

fn text_category(class: &str) -> Option<&'static str> {
    let suffix = &class[5..]; // strip "text-"
    match suffix {
        // `md` is a common non-standard size alias (the "missing" step
        // between `sm` and `lg`); it is never a color name.
        "xs" | "sm" | "md" | "base" | "lg" | "xl" => return Some("text-size"),
        "wrap" | "nowrap" | "balance" | "pretty" => return Some("text-wrap"),
        "left" | "center" | "right" | "justify" | "start" | "end" => return Some("text-align"),
        "ellipsis" | "clip" => return Some("text-overflow"),
        "uppercase" | "lowercase" | "capitalize" | "normal-case" => return Some("text-transform"),
        "underline" | "overline" | "line-through" | "no-underline" => {
            return Some("text-decoration");
        }
        _ => {}
    }
    if suffix.ends_with("xl") && suffix.len() > 2 {
        return Some("text-size");
    }
    // Arbitrary value: text-[10px] → size, text-[#fff] → color
    if suffix.starts_with('[') && suffix.ends_with(']') {
        let inner = &suffix[1..suffix.len() - 1];
        return match css_value_type(inner) {
            Some("length") => Some("text-size"),
            Some("color") => Some("text-color"),
            _ => None, // ambiguous → don't group
        };
    }
    Some("text-color")
}

fn flex_category(class: &str) -> Option<&'static str> {
    match class {
        "flex-row" | "flex-row-reverse" | "flex-col" | "flex-col-reverse" => Some("flex-direction"),
        "flex-wrap" | "flex-wrap-reverse" | "flex-nowrap" => Some("flex-wrap"),
        "flex-1" | "flex-auto" | "flex-initial" | "flex-none" => Some("flex-shorthand"),
        _ => None,
    }
}

fn border_category(class: &str) -> Option<&'static str> {
    let suffix = &class[7..]; // strip "border-"
    match suffix {
        "solid" | "dashed" | "dotted" | "double" | "hidden" | "none" => {
            return Some("border-style");
        }
        "collapse" | "separate" => return Some("border-collapse"),
        _ => {}
    }
    if suffix.chars().all(|c| c.is_ascii_digit()) {
        return Some("border-width");
    }
    for (side, group) in [
        ("t", "border-top"),
        ("r", "border-right"),
        ("b", "border-bottom"),
        ("l", "border-left"),
        ("x", "border-x"),
        ("y", "border-y"),
        ("s", "border-start"),
        ("e", "border-end"),
    ] {
        if suffix == side || suffix.starts_with(&format!("{side}-")) {
            return Some(group);
        }
    }
    Some("border-color")
}

fn font_category(class: &str) -> Option<&'static str> {
    match class {
        "font-sans" | "font-serif" | "font-mono" => Some("font-family"),
        "font-italic" | "font-not-italic" => Some("font-style"),
        "font-thin" | "font-extralight" | "font-light" | "font-normal" | "font-medium"
        | "font-semibold" | "font-bold" | "font-extrabold" | "font-black" => Some("font-weight"),
        _ => None,
    }
}

fn conflict_key(class: &str) -> Option<&'static str> {
    if class.starts_with("text-") {
        return text_category(class);
    }
    if class.starts_with("flex-") {
        return flex_category(class);
    }
    if class.starts_with("border-") {
        return border_category(class);
    }
    if class.starts_with("font-") {
        return font_category(class);
    }

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
    let mut groups: FxHashMap<&str, Vec<&str>> = FxHashMap::default();
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
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

    #[test]
    fn allows_text_size_with_text_wrap() {
        assert!(run(r#"const x = <div className="text-2xl text-balance" />;"#).is_empty());
    }

    #[test]
    fn allows_text_color_with_text_wrap() {
        assert!(
            run(r#"const x = <div className="text-muted-foreground text-pretty" />;"#).is_empty()
        );
    }

    #[test]
    fn allows_flex_shorthand_with_flex_direction() {
        assert!(run(r#"const x = <div className="flex-1 flex-col" />;"#).is_empty());
    }

    #[test]
    fn allows_border_side_with_border_color() {
        assert!(run(r#"const x = <div className="border-b border-border" />;"#).is_empty());
    }

    #[test]
    fn allows_text_sm_with_text_muted() {
        assert!(run(r#"const x = <div className="text-sm text-muted-foreground" />;"#).is_empty());
    }

    #[test]
    fn flags_two_text_sizes() {
        assert_eq!(
            run(r#"const x = <div className="text-sm text-2xl" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_two_flex_directions() {
        assert_eq!(
            run(r#"const x = <div className="flex-row flex-col" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_two_border_colors() {
        assert_eq!(
            run(r#"const x = <div className="border-red-500 border-blue-500" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_two_font_weights() {
        assert_eq!(
            run(r#"const x = <div className="font-bold font-light" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_arbitrary_size_with_color() {
        assert!(
            run(r#"const x = <div className="text-[10px] text-muted-foreground" />;"#).is_empty()
        );
    }

    #[test]
    fn flags_two_arbitrary_sizes() {
        assert_eq!(
            run(r#"const x = <div className="text-[10px] text-[1.5rem]" />;"#).len(),
            1
        );
    }

    #[test]
    fn flags_two_arbitrary_colors() {
        assert_eq!(
            run(r#"const x = <div className="text-[#ff0000] text-[#00ff00]" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_arbitrary_color_with_named_size() {
        assert!(run(r#"const x = <div className="text-[#ff0000] text-lg" />;"#).is_empty());
    }

    #[test]
    fn allows_gap_x_with_gap_y() {
        // Regression for rbaumier/comply#4072 — `gap-x-*` (column-gap) and
        // `gap-y-*` (row-gap) control different axes and are designed to
        // coexist, so they must not conflict.
        assert!(
            run(r#"const x = <div className="grid grid-cols-2 gap-x-8 gap-y-4 px-6 pb-6" />;"#)
                .is_empty()
        );
    }

    #[test]
    fn flags_conflicting_gap_x_same_axis() {
        let diags = run(r#"const x = <div className="gap-x-4 gap-x-8" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("gap-x-"));
    }

    #[test]
    fn flags_conflicting_gap_y_same_axis() {
        let diags = run(r#"const x = <div className="gap-y-2 gap-y-6" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("gap-y-"));
    }

    #[test]
    fn flags_conflicting_gap_shorthand() {
        let diags = run(r#"const x = <div className="gap-2 gap-6" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("gap-2, gap-6"));
    }
}
