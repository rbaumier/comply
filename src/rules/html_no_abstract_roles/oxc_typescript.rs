//! html-no-abstract-roles — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue};
use std::sync::Arc;

pub struct Check;

const ABSTRACT_ROLES: &[&str] = &[
    "command",
    "composite",
    "input",
    "landmark",
    "range",
    "roletype",
    "section",
    "sectionhead",
    "select",
    "structure",
    "widget",
    "window",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["role"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let JSXAttributeName::Identifier(name) = &attr.name else { continue };
            if name.name.as_str() != "role" {
                continue;
            }
            let Some(JSXAttributeValue::StringLiteral(val)) = &attr.value else {
                continue;
            };
            let role = val.value.as_str();
            if !ABSTRACT_ROLES.contains(&role) {
                continue;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, attr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("Abstract ARIA role `{role}` must not be used on DOM elements."),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }


    #[test]
    fn flags_abstract_role_widget() {
        let d = run(r#"const x = <div role="widget" />;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("widget"));
    }


    #[test]
    fn flags_abstract_role_section() {
        assert_eq!(run(r#"const x = <div role="section" />;"#).len(), 1);
    }


    #[test]
    fn flags_abstract_role_range() {
        assert_eq!(run(r#"const x = <div role="range" />;"#).len(), 1);
    }


    #[test]
    fn allows_concrete_role() {
        assert!(run(r#"const x = <div role="button" />;"#).is_empty());
    }


    #[test]
    fn allows_navigation_role() {
        assert!(run(r#"const x = <nav role="navigation" />;"#).is_empty());
    }
}
