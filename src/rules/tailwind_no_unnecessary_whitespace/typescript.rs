//! tailwind-no-unnecessary-whitespace — flag consecutive spaces inside
//! `className`/`class` attribute values.
//!
//! Walks JSX `jsx_attribute` nodes (TS/TSX/JS) and Vue `attribute` nodes
//! (Vue SFC `<template>`). For each `class`/`className` attribute, extracts
//! the string value and reports if it contains two or more consecutive
//! spaces.

use crate::diagnostic::{Diagnostic, Severity};

/// True when `s` contains two or more consecutive space characters.
fn has_consecutive_spaces(s: &str) -> bool {
    s.as_bytes()
        .windows(2)
        .any(|w| w[0] == b' ' && w[1] == b' ')
}

/// Extract the (name, value) pair from a JSX `jsx_attribute` node, returning
/// the unquoted string value. Returns `None` if the value isn't a literal
/// string or the attribute isn't named `className`/`class`.
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

/// Extract the value of a Vue `attribute` node when its name is `class`.
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
    if has_consecutive_spaces(class_str) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Unnecessary whitespace in class string — collapse consecutive spaces.".into(),
            Severity::Warning,
        ));
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
    fn flags_double_space_in_classname() {
        let diags = run(r#"const x = <div className="p-4  mt-2" />;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_double_space_in_class_attr() {
        let diags = run(r#"const x = <div class="text-lg   font-bold" />;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_single_spaces() {
        assert!(run(r#"const x = <div className="p-4 mt-2 text-lg" />;"#).is_empty());
    }

    #[test]
    fn allows_empty_class() {
        assert!(run(r#"const x = <div className="" />;"#).is_empty());
    }

    #[test]
    fn flags_multiple_attributes_on_same_line() {
        let diags = run(r#"const x = <div className="a  b" class="c  d" />;"#);
        assert_eq!(diags.len(), 2);
    }
}
