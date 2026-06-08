//! tailwind-no-magic-spacing — flag arbitrary pixel spacing utilities
//! (`p-[13px]`, `gap-[7px]`) whose numeric value is not a multiple of 4.
//!
//! Walks JSX `jsx_attribute` nodes (TS/TSX/JS) and Vue `attribute` nodes
//! (Vue SFC `<template>`). For each `class`/`className` attribute, checks
//! every whitespace-separated token against the spacing-prefix set and
//! parses the bracketed value as a `<digits>px` literal. Multiples of 4
//! pass; everything else (and any non-pixel unit) is left alone.

use crate::diagnostic::{Diagnostic, Severity};

const SPACING_PREFIXES: &[&str] = &[
    "p-[",
    "px-[",
    "py-[",
    "pt-[",
    "pb-[",
    "pl-[",
    "pr-[",
    "m-[",
    "mx-[",
    "my-[",
    "mt-[",
    "mb-[",
    "ml-[",
    "mr-[",
    "gap-[",
    "gap-x-[",
    "gap-y-[",
    "space-x-[",
    "space-y-[",
];

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

/// Parse a value like `13px` as `Some(13)`. Anything that does not end in
/// `px` with only digits before it returns `None`.
fn parse_px(value: &str) -> Option<u64> {
    let stripped = value.strip_suffix("px")?;
    if stripped.is_empty() {
        return None;
    }
    stripped.parse::<u64>().ok()
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let class_str = jsx_class_value(node, source)
        .or_else(|| vue_class_value(node, source));
    let Some(class_str) = class_str else { return; };
    for token in class_str.split_whitespace() {
        for prefix in SPACING_PREFIXES {
            if let Some(rest) = token.strip_prefix(prefix)
                && let Some(value) = rest.strip_suffix(']')
                && let Some(n) = parse_px(value)
                && n % 4 != 0
            {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    format!(
                        "`{}{value}]` uses {n}px which is not a multiple of 4 — stick to the design-token spacing scale.",
                        prefix.trim_end_matches('[')
                    ),
                    Severity::Warning,
                ));
                break;
            }
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_non_multiple_of_four_padding() {
        assert_eq!(run(r#"const x = <div className="p-[13px]" />;"#).len(), 1);
    }

    #[test]
    fn flags_margin_seven() {
        assert_eq!(run(r#"const x = <div className="m-[7px]" />;"#).len(), 1);
    }

    #[test]
    fn flags_gap_eleven() {
        assert_eq!(run(r#"const x = <div className="gap-[11px]" />;"#).len(), 1);
    }

    #[test]
    fn allows_multiple_of_four() {
        assert!(run(r#"const x = <div className="p-[16px]" />;"#).is_empty());
    }

    #[test]
    fn allows_rem_unit() {
        assert!(run(r#"const x = <div className="p-[1.5rem]" />;"#).is_empty());
    }

    #[test]
    fn allows_standard_scale() {
        assert!(run(r#"const x = <div className="p-4" />;"#).is_empty());
    }
}
