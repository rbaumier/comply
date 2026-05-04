//! a11y-heading-has-content OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXChild, JSXElementName};
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        let JSXElementName::Identifier(tag_ident) = &opening.name else {
            return;
        };
        let tag = tag_ident.name.as_str();
        if !matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
            return;
        }

        // Walk up to the parent JSXElement to inspect children.
        let Some(parent) = semantic.nodes().ancestors(node.id()).nth(1) else {
            return;
        };
        let AstKind::JSXElement(element) = parent.kind() else {
            return;
        };

        let has_content = element.children.iter().any(|child| match child {
            JSXChild::Text(text) => !text.value.trim().is_empty(),
            _ => true,
        });

        if !has_content {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            // Determine if self-closing by checking if closing_element is absent.
            let msg = if element.closing_element.is_none() {
                format!("`<{tag}>` is self-closing and has no content.")
            } else {
                format!("`<{tag}>` is empty and has no content.")
            };
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: msg,
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
