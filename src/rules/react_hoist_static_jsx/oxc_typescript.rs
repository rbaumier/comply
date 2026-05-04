//! OxcCheck backend for react-hoist-static-jsx.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, JSXAttributeItem, JSXAttributeValue,
    JSXChild, JSXElementName};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn jsx_element_name_is_uppercase(name: &JSXElementName) -> bool {
    match name {
        JSXElementName::Identifier(id) => starts_with_uppercase(id.name.as_str()),
        JSXElementName::IdentifierReference(id) => starts_with_uppercase(id.name.as_str()),
        _ => true,
    }
}

/// Recursive: true when the JSX element has no expression containers and no
/// uppercase-named JSX tags.
fn is_static_jsx_element(elem: &oxc_ast::ast::JSXElement) -> bool {
    if jsx_element_name_is_uppercase(&elem.opening_element.name) {
        return false;
    }

    for attr in &elem.opening_element.attributes {
        match attr {
            JSXAttributeItem::Attribute(a) => {
                if let Some(JSXAttributeValue::ExpressionContainer(_)) = &a.value {
                    return false;
                }
            }
            JSXAttributeItem::SpreadAttribute(_) => return false,
        }
    }

    for child in &elem.children {
        if !is_static_jsx_child(child) {
            return false;
        }
    }
    true
}

fn is_static_jsx_child(child: &JSXChild) -> bool {
    match child {
        JSXChild::ExpressionContainer(_) => false,
        JSXChild::Element(elem) => is_static_jsx_element(elem),
        JSXChild::Fragment(frag) => frag.children.iter().all(|c| is_static_jsx_child(c)),
        JSXChild::Spread(_) => false,
        JSXChild::Text(_) => true,
    }
}

/// True when `node` lives inside a PascalCase component function body.
fn inside_component_body<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()).skip(1) {
        match ancestor.kind() {
            AstKind::Function(func) => {
                if let Some(id) = &func.id {
                    return starts_with_uppercase(id.name.as_str());
                }
            }
            AstKind::ArrowFunctionExpression(_) => {
                if let Some(grandparent) = nodes.ancestors(ancestor.id()).nth(1) {
                    if let AstKind::VariableDeclarator(decl) = grandparent.kind() {
                        if let BindingPattern::BindingIdentifier(id) = &decl.id {
                            return starts_with_uppercase(id.name.as_str());
                        }
                    }
                }
            }
            _ => continue,
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclarator(decl) = node.kind() else {
            return;
        };
        let Some(init) = &decl.init else { return };

        let span = match init {
            Expression::JSXElement(elem) => {
                if !is_static_jsx_element(elem) {
                    return;
                }
                elem.span()
            }
            _ => return,
        };

        if !inside_component_body(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Static JSX inside a component is rebuilt every render. \
                      Move this element to a module-level `const` above the \
                      component so it's built once."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
