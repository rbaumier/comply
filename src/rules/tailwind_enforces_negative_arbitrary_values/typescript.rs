//! tailwind-enforces-negative-arbitrary-values — flag Tailwind arbitrary
//! values written with a leading minus on the utility prefix
//! (`-top-[1px]`) instead of inside the brackets (`top-[-1px]`).
//!
//! Walks JSX `jsx_attribute` nodes (TS/TSX/JS) and Vue `attribute` nodes
//! (Vue SFC `<template>`). For each `class`/`className` attribute, scans
//! every whitespace-separated token: a token that starts with `-` followed
//! by a known prop (`top`, `m`, `mt`, …) and an arbitrary value whose
//! first inner character is not `-` is reported.

use crate::diagnostic::{Diagnostic, Severity};

const NEGATABLE_PROPS: &[&str] = &[
    "top",
    "bottom",
    "left",
    "right",
    "m",
    "mt",
    "mb",
    "ml",
    "mr",
    "mx",
    "my",
    "p",
    "pt",
    "pb",
    "pl",
    "pr",
    "px",
    "py",
    "inset",
    "translate",
    "rotate",
    "skew",
    "scale",
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

/// Returns `true` if `token` matches the shape `-<prop>-[<value>…]` where
/// `<prop>` is in `NEGATABLE_PROPS` and `<value>` does NOT itself start
/// with `-`.
fn is_negative_prefix_arbitrary(token: &str) -> bool {
    let Some(rest) = token.strip_prefix('-') else {
        return false;
    };
    for prop in NEGATABLE_PROPS {
        let needle = format!("{prop}-[");
        if let Some(after_bracket) = rest.strip_prefix(&needle)
            && !after_bracket.is_empty()
            && !after_bracket.starts_with('-')
            && after_bracket.contains(']')
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
    for token in class_str.split_whitespace() {
        if is_negative_prefix_arbitrary(token) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "Move the minus inside the brackets (e.g. `top-[-1px]` instead of `-top-[1px]`).".into(),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_negative_prefix_on_top() {
        assert_eq!(run(r#"const x = <div className="-top-[1px]" />;"#).len(), 1);
    }

    #[test]
    fn flags_negative_prefix_on_margin() {
        assert_eq!(
            run(r#"const x = <div className="-mt-[4px] -ml-[2rem]" />;"#).len(),
            2
        );
    }

    #[test]
    fn allows_value_inside_brackets() {
        assert!(run(r#"const x = <div className="top-[-1px]" />;"#).is_empty());
    }

    #[test]
    fn allows_non_arbitrary_negative_utility() {
        assert!(run(r#"const x = <div className="-m-4 -top-2" />;"#).is_empty());
    }

    #[test]
    fn allows_positive_arbitrary() {
        assert!(run(r#"const x = <div className="top-[1px] m-[4px]" />;"#).is_empty());
    }

    #[test]
    fn flags_in_vue_class_attribute() {
        assert_eq!(run(r#"const x = <div class="-inset-[2px]" />;"#).len(), 1);
    }
}
