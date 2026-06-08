//! tailwind-no-duplicate-classes — flag duplicate CSS classes inside
//! `className`/`class` attributes.
//!
//! Walks JSX `jsx_attribute` nodes (TS/TSX/JS) and Vue `attribute` nodes
//! (Vue SFC `<template>`). For each `class`/`className` attribute, splits
//! the value on whitespace and reports any token that appears more than
//! once.

use std::collections::HashSet;

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

crate::ast_check! { |node, source, ctx, diagnostics|
    let class_str = jsx_class_value(node, source)
        .or_else(|| vue_class_value(node, source));
    let Some(class_str) = class_str else { return; };
    let mut seen: HashSet<&str> = HashSet::new();
    for class in class_str.split_whitespace() {
        if !seen.insert(class) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!("Duplicate class `{class}` — remove the repetition."),
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
    fn flags_duplicate_classname() {
        let diags = run(r#"const x = <div className="p-4 mt-2 p-4" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("p-4"));
    }

    #[test]
    fn flags_duplicate_class_attr() {
        let diags = run(r#"const x = <div class="text-lg text-lg" />;"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("text-lg"));
    }

    #[test]
    fn allows_unique_classes() {
        assert!(run(r#"const x = <div className="p-4 mt-2 text-lg" />;"#).is_empty());
    }

    #[test]
    fn flags_multiple_duplicates() {
        let diags = run(r#"const x = <div className="p-4 mt-2 p-4 mt-2" />;"#);
        assert_eq!(diags.len(), 2);
    }
}
