//! react-no-namespace oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXElementName};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
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

        // Check element name for namespace.
        if let JSXElementName::NamespacedName(ns) = &opening.name {
            let name = format!("{}:{}", ns.namespace.name, ns.name.name);
            let (line, column) =
                byte_offset_to_line_col(ctx.source, ns.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Namespaced JSX element `{name}` is not supported by React."
                ),
                severity: Severity::Error,
                span: None,
            });
        }

        // Check attributes for namespace.
        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            if let JSXAttributeName::NamespacedName(ns) = &attr.name {
                let name = format!("{}:{}", ns.namespace.name, ns.name.name);
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, ns.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Namespaced JSX attribute `{name}` is not supported by React."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
    }
}
