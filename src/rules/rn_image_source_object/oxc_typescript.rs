//! OxcCheck backend for rn-image-source-object.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeItem;
use oxc_ast::ast::JSXAttributeValue;
use oxc_ast::ast::JSXElementName;
use oxc_span::GetSpan;
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
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        let tag = match &opening.name {
            JSXElementName::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if tag != "Image" {
            return;
        }

        for attr in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr else { continue };
            let attr_name = match &attr.name {
                oxc_ast::ast::JSXAttributeName::Identifier(id) => id.name.as_str(),
                _ => continue,
            };
            if attr_name != "source" {
                continue;
            }
            if let Some(JSXAttributeValue::StringLiteral(s)) = &attr.value {
                let span = s.span();
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`<Image source=\"...\">` with a string literal renders nothing — use `{{ uri: '...' }}` or `require(...)`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
