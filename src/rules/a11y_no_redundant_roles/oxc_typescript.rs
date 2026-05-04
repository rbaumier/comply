//! a11y-no-redundant-roles OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName};
use std::sync::Arc;

/// (tag, redundant implicit role)
const REDUNDANT_PAIRS: &[(&str, &str)] = &[
    ("button", "button"),
    ("nav", "navigation"),
    ("img", "img"),
    ("input", "textbox"),
    ("h1", "heading"),
    ("h2", "heading"),
    ("h3", "heading"),
    ("h4", "heading"),
    ("h5", "heading"),
    ("h6", "heading"),
    ("ul", "list"),
    ("ol", "list"),
    ("li", "listitem"),
    ("table", "table"),
    ("form", "form"),
];

pub struct Check;

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
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        let JSXElementName::Identifier(tag_ident) = &opening.name else {
            return;
        };
        let tag = tag_ident.name.as_str();

        let mut role_value: Option<&str> = None;
        let mut has_href = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            let name = name_ident.name.as_str();
            if name == "role" {
                if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value {
                    role_value = Some(lit.value.as_str());
                }
            }
            if name == "href" {
                has_href = true;
            }
        }

        let Some(role) = role_value else { return };

        // Check standard redundant pairs
        for &(pair_tag, pair_role) in REDUNDANT_PAIRS {
            if tag == pair_tag && role == pair_role {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, opening.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "The element `<{tag}>` has an implicit role of `{pair_role}`. Setting `role=\"{pair_role}\"` is redundant."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }
        }

        // Special case: <a href="..." role="link"> is redundant
        if tag == "a" && has_href && role == "link" {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "The element `<a>` with `href` has an implicit role of `link`. Setting `role=\"link\"` is redundant.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
