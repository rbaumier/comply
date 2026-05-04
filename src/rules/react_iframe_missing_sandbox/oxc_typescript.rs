//! react-iframe-missing-sandbox oxc backend.

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

        let tag = match &opening.name {
            JSXElementName::Identifier(ident) => ident.name.as_str(),
            _ => return,
        };

        if tag != "iframe" {
            return;
        }

        let has_sandbox = opening.attributes.iter().any(|attr_item| {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                return false;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                return false;
            };
            name_ident.name.as_str() == "sandbox"
        });

        if !has_sandbox {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`<iframe>` without a `sandbox` attribute can access \
                          the parent page. Add `sandbox` to restrict its \
                          capabilities."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
