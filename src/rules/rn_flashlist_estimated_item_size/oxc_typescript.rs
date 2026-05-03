//! rn-flashlist-estimated-item-size oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["FlashList"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(el) = node.kind() else { return };
        let oxc_ast::ast::JSXElementName::Identifier(ident) = &el.name else { return };
        if ident.name.as_str() != "FlashList" {
            return;
        }
        let has_attr = el.attributes.iter().any(|attr| {
            if let oxc_ast::ast::JSXAttributeItem::Attribute(a) = attr {
                if let oxc_ast::ast::JSXAttributeName::Identifier(id) = &a.name {
                    return id.name.as_str() == "estimatedItemSize";
                }
            }
            false
        });
        if has_attr {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, el.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`<FlashList>` is missing `estimatedItemSize` — required for performance."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
