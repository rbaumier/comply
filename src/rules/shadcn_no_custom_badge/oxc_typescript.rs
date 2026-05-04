use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName};
use std::sync::Arc;

fn looks_like_badge(value: &str) -> bool {
    let mut has_rounded_full = false;
    let mut has_bg = false;
    for class in value.split_ascii_whitespace() {
        let util = class
            .rsplit(':')
            .next()
            .unwrap_or(class)
            .trim_start_matches('!');
        if util == "rounded-full" {
            has_rounded_full = true;
        }
        if util.starts_with("bg-") {
            has_bg = true;
        }
    }
    has_rounded_full && has_bg
}

fn jsx_tag_name<'a>(opening: &'a oxc_ast::ast::JSXOpeningElement<'a>) -> Option<&'a str> {
    match &opening.name {
        JSXElementName::Identifier(id) => Some(id.name.as_str()),
        JSXElementName::IdentifierReference(id) => Some(id.name.as_str()),
        _ => None,
    }
}

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
        let Some(tag) = jsx_tag_name(opening) else {
            return;
        };
        if tag != "span" {
            return;
        }

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            if name.name.as_str() != "className" {
                continue;
            }
            let Some(JSXAttributeValue::StringLiteral(s)) = &attr.value else {
                continue;
            };
            if looks_like_badge(s.value.as_str()) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, opening.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Home-grown badge detected \u{2014} use `<Badge>` from shadcn/ui instead of `<span className=\"rounded-full bg-\u{2026}\">`."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }
        }
    }
}
