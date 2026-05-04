//! jsx-fragments OXC backend — flag `<React.Fragment>` or bare `<Fragment>`
//! opening elements, except when a `key` prop forces the long form.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXElementName};
use std::sync::Arc;

pub struct Check;

fn is_fragment_tag(name: &JSXElementName) -> bool {
    match name {
        JSXElementName::Identifier(id) => id.name.as_str() == "Fragment",
        JSXElementName::IdentifierReference(id) => id.name.as_str() == "Fragment",
        JSXElementName::MemberExpression(member) => {
            if member.property.name.as_str() != "Fragment" {
                return false;
            }
            match &member.object {
                oxc_ast::ast::JSXMemberExpressionObject::IdentifierReference(id) => {
                    id.name.as_str() == "React"
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn has_key_attribute(attrs: &oxc_allocator::Vec<'_, JSXAttributeItem<'_>>) -> bool {
    attrs.iter().any(|item| {
        if let JSXAttributeItem::Attribute(attr) = item
            && let JSXAttributeName::Identifier(id) = &attr.name {
                return id.name.as_str() == "key";
            }
        false
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Fragment"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };
        if !is_fragment_tag(&opening.name) {
            return;
        }
        if has_key_attribute(&opening.attributes) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer the short fragment syntax `<>...</>`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
