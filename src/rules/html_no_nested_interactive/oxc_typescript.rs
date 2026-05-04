use std::sync::Arc;

use oxc_ast::AstKind;
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXChild, JSXElementName,
    JSXExpression, UnaryOperator,
};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};

const INTERACTIVE_ELEMENTS: &[&str] = &["button", "a", "input", "select", "textarea", "details"];

const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "link",
    "checkbox",
    "radio",
    "switch",
    "tab",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "combobox",
    "listbox",
    "slider",
    "spinbutton",
    "textbox",
    "searchbox",
    "treeitem",
];

fn element_name_str<'a>(name: &'a JSXElementName<'a>) -> Option<&'a str> {
    match name {
        JSXElementName::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

fn is_interactive_by_tag(name: &str) -> bool {
    INTERACTIVE_ELEMENTS.contains(&name.to_lowercase().as_str())
}

fn has_interactive_role(attrs: &oxc_allocator::Vec<'_, JSXAttributeItem>) -> bool {
    for item in attrs {
        let JSXAttributeItem::Attribute(attr) = item else {
            continue;
        };
        let JSXAttributeName::Identifier(name) = &attr.name else {
            continue;
        };
        if name.name.as_str() != "role" {
            continue;
        }
        if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value
            && INTERACTIVE_ROLES.contains(&lit.value.as_str()) {
                return true;
            }
    }
    false
}

fn has_tabindex(attrs: &oxc_allocator::Vec<'_, JSXAttributeItem>) -> bool {
    for item in attrs {
        let JSXAttributeItem::Attribute(attr) = item else {
            continue;
        };
        let JSXAttributeName::Identifier(name) = &attr.name else {
            continue;
        };
        let n = name.name.as_str();
        if n != "tabIndex" && n != "tabindex" {
            continue;
        }
        match &attr.value {
            Some(JSXAttributeValue::StringLiteral(lit)) => {
                return lit.value.as_str() != "-1";
            }
            Some(JSXAttributeValue::ExpressionContainer(container)) => {
                if let JSXExpression::NumericLiteral(num) = &container.expression {
                    return num.value != -1.0;
                }
                if let JSXExpression::UnaryExpression(unary) = &container.expression {
                    if unary.operator == UnaryOperator::UnaryNegation
                        && let oxc_ast::ast::Expression::NumericLiteral(num) = &unary.argument
                            && num.value == 1.0 {
                                return false;
                            }
                    return true;
                }
                return true;
            }
            _ => return false,
        }
    }
    false
}

fn is_interactive_opening(
    name: &JSXElementName,
    attrs: &oxc_allocator::Vec<'_, JSXAttributeItem>,
) -> bool {
    if let Some(tag) = element_name_str(name)
        && is_interactive_by_tag(tag) {
            return true;
        }
    has_interactive_role(attrs) || has_tabindex(attrs)
}

/// Recursively search children for a nested interactive element.
/// Returns the span start of the first found nested interactive.
fn find_nested_interactive(children: &oxc_allocator::Vec<'_, JSXChild>) -> Option<u32> {
    for child in children {
        match child {
            JSXChild::Element(el) => {
                if is_interactive_opening(&el.opening_element.name, &el.opening_element.attributes)
                {
                    return Some(el.span.start);
                }
                if let Some(pos) = find_nested_interactive(&el.children) {
                    return Some(pos);
                }
            }
            JSXChild::Fragment(frag) => {
                if let Some(pos) = find_nested_interactive(&frag.children) {
                    return Some(pos);
                }
            }
            JSXChild::ExpressionContainer(_) | JSXChild::Spread(_) | JSXChild::Text(_) => {}
        }
    }
    None
}

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
        if !is_interactive_opening(
            &element.opening_element.name,
            &element.opening_element.attributes,
        ) {
            return;
        }
        if let Some(nested_start) = find_nested_interactive(&element.children) {
            let (line, column) = byte_offset_to_line_col(ctx.source, nested_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Interactive element is nested inside another interactive element.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
