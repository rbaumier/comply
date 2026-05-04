//! shadcn-no-manual-zindex-overlays OxcCheck backend — forbid `z-*` on
//! shadcn overlay primitives in JSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName,
    JSXMemberExpressionObject,
};
use std::sync::Arc;

const OVERLAY_TAGS: &[&str] = &[
    "Dialog",
    "DialogContent",
    "DialogOverlay",
    "Sheet",
    "SheetContent",
    "SheetOverlay",
    "Drawer",
    "DrawerContent",
    "DrawerOverlay",
    "AlertDialog",
    "AlertDialogContent",
    "AlertDialogOverlay",
    "DropdownMenu",
    "DropdownMenuContent",
    "Popover",
    "PopoverContent",
    "Tooltip",
    "TooltipContent",
];

fn is_zindex_class(class: &str) -> bool {
    let utility = class.rsplit(':').next().unwrap_or(class);
    let utility = utility.trim_start_matches('!').trim_start_matches('-');
    let Some(rest) = utility.strip_prefix("z-") else {
        return false;
    };
    if rest == "auto" {
        return false;
    }
    rest.chars()
        .all(|c| c.is_ascii_digit() || c == '[' || c == ']')
        && rest.chars().any(|c| c.is_ascii_digit())
}

fn tag_is_overlay(tag: &str) -> bool {
    if OVERLAY_TAGS.contains(&tag) {
        return true;
    }
    let first = tag.split('.').next().unwrap_or(tag);
    OVERLAY_TAGS.contains(&first)
}

fn jsx_element_name(name: &JSXElementName) -> Option<String> {
    match name {
        JSXElementName::Identifier(id) => Some(id.name.to_string()),
        JSXElementName::IdentifierReference(id) => Some(id.name.to_string()),
        JSXElementName::MemberExpression(member) => {
            let obj_name = match &member.object {
                JSXMemberExpressionObject::IdentifierReference(id) => id.name.to_string(),
                JSXMemberExpressionObject::MemberExpression(inner) => {
                    match &inner.object {
                        JSXMemberExpressionObject::IdentifierReference(id) => {
                            id.name.to_string()
                        }
                        _ => return None,
                    }
                }
                JSXMemberExpressionObject::ThisExpression(_) => return None,
            };
            Some(format!("{}.{}", obj_name, member.property.name))
        }
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
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        let Some(tag) = jsx_element_name(&opening.name) else { return };
        if !tag_is_overlay(&tag) {
            return;
        }

        for attr in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr else { continue };
            let JSXAttributeName::Identifier(name) = &attr.name else { continue };
            if name.name != "className" {
                continue;
            }
            let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value else { continue };
            let value = lit.value.as_str();
            if value.split_ascii_whitespace().any(is_zindex_class) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`z-*` on `{tag}` fights shadcn's overlay stacking — drop the z-index utility."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
