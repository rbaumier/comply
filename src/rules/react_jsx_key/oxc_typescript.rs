//! react-jsx-key OxcCheck backend.
//!
//! Flags JSX elements inside `.map()` / `.flatMap()` / `.from()` callbacks and
//! array literals that lack a `key` prop.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, JSXAttributeItem, JSXAttributeName};
use oxc_span::GetSpan;
use std::sync::Arc;

fn has_key_prop(opening: &oxc_ast::ast::JSXOpeningElement) -> bool {
    opening.attributes.iter().any(|attr_item| {
        let JSXAttributeItem::Attribute(attr) = attr_item else {
            return false;
        };
        let JSXAttributeName::Identifier(name_ident) = &attr.name else {
            return false;
        };
        name_ident.name.as_str() == "key"
    })
}

fn is_in_iterator<'a>(
    node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();

    let mut current_id = node_id;
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);

        match parent.kind() {
            AstKind::ArrayExpression(_) => return true,
            AstKind::ParenthesizedExpression(_)
            | AstKind::JSXExpressionContainer(_)
            | AstKind::ReturnStatement(_)
            | AstKind::ExpressionStatement(_) => {
                current_id = parent_id;
            }
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                // Walk up from the function to find the CallExpression
                // Pattern: Function -> (FormalParameters?) -> CallExpression
                let mut up_id = parent_id;
                loop {
                    let next_id = nodes.parent_id(up_id);
                    if next_id == up_id {
                        return false;
                    }
                    let next = nodes.get_node(next_id);
                    match next.kind() {
                        AstKind::CallExpression(call) => {
                            let Expression::StaticMemberExpression(member) = &call.callee else {
                                return false;
                            };
                            let method = member.property.name.as_str();
                            return matches!(method, "map" | "flatMap" | "from");
                        }
                        _ => {
                            up_id = next_id;
                        }
                    }
                }
            }
            _ => return false,
        }
    }
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXElement(element) = node.kind() else {
            return;
        };

        if has_key_prop(&element.opening_element) {
            return;
        }

        if is_in_iterator(node.id(), semantic) {
            let (line, column) = byte_offset_to_line_col(
                ctx.source,
                element.opening_element.span().start as usize,
            );
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Missing `key` prop for JSX element in iterator — \
                          React needs stable keys to reconcile lists."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
