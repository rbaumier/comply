use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeItem;
use std::sync::Arc;

pub struct Check;

fn tag_is_dialog(tag: &str) -> bool {
    tag == "dialog" || tag == "Dialog" || tag.ends_with("Dialog") || tag == "AlertDialog"
}

fn jsx_tag_name<'a>(opening: &'a oxc_ast::ast::JSXOpeningElement<'a>) -> Option<&'a str> {
    match &opening.name {
        oxc_ast::ast::JSXElementName::Identifier(id) => Some(id.name.as_str()),
        oxc_ast::ast::JSXElementName::IdentifierReference(id) => Some(id.name.as_str()),
        oxc_ast::ast::JSXElementName::NamespacedName(ns) => Some(ns.name.name.as_str()),
        oxc_ast::ast::JSXElementName::MemberExpression(member) => {
            Some(member.property.name.as_str())
        }
        _ => None,
    }
}

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

        let mut role_dialog = false;
        let mut has_label = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let oxc_ast::ast::JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            match name.name.as_str() {
                "role" => {
                    if let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(s)) = &attr.value
                        && (s.value.as_str() == "dialog" || s.value.as_str() == "alertdialog") {
                            role_dialog = true;
                        }
                }
                "aria-label" | "aria-labelledby" => {
                    has_label = true;
                }
                _ => {}
            }
        }

        let is_dialog = tag_is_dialog(tag) || role_dialog;
        if !is_dialog {
            return;
        }
        if has_label {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`<{tag}>` is a dialog but has no `aria-label` or `aria-labelledby` — screen readers cannot name it."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
