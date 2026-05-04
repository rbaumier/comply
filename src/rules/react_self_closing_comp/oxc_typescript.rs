//! react-self-closing-comp oxc backend.
//!
//! Flags `<Foo></Foo>` or `<div></div>` when there are no children.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXChild, JSXElementName};
use std::sync::Arc;

/// HTML void elements that must always self-close (never flagged).
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXElement(element) = node.kind() else {
            return;
        };

        // Must have a closing element (not self-closing).
        if element.closing_element.is_none() {
            return;
        }

        // Get tag name.
        let tag = match &element.opening_element.name {
            JSXElementName::Identifier(id) => id.name.as_str(),
            JSXElementName::IdentifierReference(id) => id.name.as_str(),
            JSXElementName::MemberExpression(m) => m.property.name.as_str(),
            _ => return,
        };

        // Skip void elements.
        if VOID_ELEMENTS.contains(&tag) {
            return;
        }

        // Check if there are any meaningful children.
        let has_children = element.children.iter().any(|child| match child {
            JSXChild::Text(text) => !text.value.trim().is_empty(),
            _ => true,
        });

        if !has_children {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, element.opening_element.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`<{tag}></{tag}>` has no children \u{2014} use `<{tag} />` instead."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
