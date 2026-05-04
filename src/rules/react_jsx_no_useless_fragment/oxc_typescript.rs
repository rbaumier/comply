//! react-jsx-no-useless-fragment OxcCheck backend.
//!
//! Flags `<Fragment>` / `<React.Fragment>` wrapping zero or one child.
//! Also handles `<></>` (JSXFragment).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXChild, JSXElementName, JSXMemberExpressionObject};
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
            matches!(&member.object, JSXMemberExpressionObject::IdentifierReference(obj) if obj.name.as_str() == "React")
        }
        _ => false,
    }
}

fn is_meaningful_child(child: &JSXChild) -> bool {
    match child {
        JSXChild::Text(text) => !text.value.trim().is_empty(),
        _ => true,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement, AstType::JSXFragment]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Fragment", "<>"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::JSXOpeningElement(opening) => {
                if !is_fragment_tag(&opening.name) {
                    return;
                }
                // Walk up to the parent JSXElement to count children.
                let Some(parent) = semantic.nodes().ancestors(node.id()).nth(1) else {
                    return;
                };
                let AstKind::JSXElement(element) = parent.kind() else {
                    return;
                };
                let meaningful = element
                    .children
                    .iter()
                    .filter(|c| is_meaningful_child(c))
                    .count();
                if meaningful <= 1 {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, element.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Unnecessary fragment \u{2014} a fragment wrapping zero or one \
                                  child adds no value."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::JSXFragment(frag) => {
                let meaningful = frag
                    .children
                    .iter()
                    .filter(|c| is_meaningful_child(c))
                    .count();
                if meaningful <= 1 {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, frag.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Unnecessary fragment \u{2014} a fragment wrapping zero or one \
                                  child adds no value."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
    }
}
