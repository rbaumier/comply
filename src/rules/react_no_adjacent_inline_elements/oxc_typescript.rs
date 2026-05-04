//! react-no-adjacent-inline-elements OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXChild, JSXElementName};
use oxc_span::GetSpan;
use std::sync::Arc;

/// Common inline HTML elements.
const INLINE_ELEMENTS: &[&str] = &[
    "a", "abbr", "b", "bdi", "bdo", "br", "cite", "code", "data", "dfn", "em", "i", "kbd",
    "mark", "q", "rp", "rt", "ruby", "s", "samp", "small", "span", "strong", "sub", "sup",
    "time", "u", "var", "wbr", "img", "input", "button", "label", "select", "textarea",
];

fn is_inline_element(name: &JSXElementName) -> bool {
    match name {
        JSXElementName::Identifier(id) => {
            let tag = id.name.as_str();
            // PascalCase components are inline by default.
            if tag.starts_with(|c: char| c.is_ascii_uppercase()) {
                return true;
            }
            INLINE_ELEMENTS.contains(&tag)
        }
        JSXElementName::IdentifierReference(id) => {
            id.name.starts_with(|c: char| c.is_ascii_uppercase())
        }
        JSXElementName::MemberExpression(_) => true,
        _ => false,
    }
}

fn is_inline_child(child: &JSXChild) -> bool {
    match child {
        JSXChild::Element(el) => is_inline_element(&el.opening_element.name),
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXElement, AstType::JSXFragment]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let children: &[JSXChild] = match node.kind() {
            AstKind::JSXElement(el) => &el.children,
            AstKind::JSXFragment(frag) => &frag.children,
            _ => return,
        };

        let mut i = 0;
        while i + 1 < children.len() {
            let child_a = &children[i];
            let child_b = &children[i + 1];

            if is_inline_child(child_a) && is_inline_child(child_b) {
                let a_end = child_a.span().end as usize;
                let b_start = child_b.span().start as usize;
                let between = &ctx.source[a_end..b_start];

                if between.is_empty() {
                    let (line, column) = byte_offset_to_line_col(ctx.source, b_start);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Adjacent inline elements without whitespace — \
                                  add `{' '}` or a wrapper."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            i += 1;
        }
    }
}
