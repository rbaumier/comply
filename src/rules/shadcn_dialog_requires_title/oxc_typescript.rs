//! shadcn-dialog-requires-title OXC backend — each `<DialogContent>` must
//! contain a `<DialogTitle>` descendant.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXChild, JSXElementName};
use std::sync::Arc;

pub struct Check;

fn tag_matches(tag: &str, flat: &str, dotted_suffix: &str) -> bool {
    tag == flat || tag.ends_with(dotted_suffix)
}

fn jsx_opening_tag_name<'a>(name: &'a JSXElementName<'a>) -> Option<String> {
    match name {
        JSXElementName::Identifier(id) => Some(id.name.to_string()),
        JSXElementName::IdentifierReference(id) => Some(id.name.to_string()),
        JSXElementName::MemberExpression(member) => {
            let mut parts = vec![member.property.name.as_str()];
            let mut obj = &member.object;
            loop {
                match obj {
                    oxc_ast::ast::JSXMemberExpressionObject::IdentifierReference(id) => {
                        parts.push(id.name.as_str());
                        break;
                    }
                    oxc_ast::ast::JSXMemberExpressionObject::MemberExpression(m) => {
                        parts.push(m.property.name.as_str());
                        obj = &m.object;
                    }
                    _ => return None,
                }
            }
            parts.reverse();
            Some(parts.join("."))
        }
        _ => None,
    }
}

fn children_have_title(children: &oxc_allocator::Vec<'_, JSXChild<'_>>) -> bool {
    for child in children.iter() {
        match child {
            JSXChild::Element(el) => {
                let tag = jsx_opening_tag_name(&el.opening_element.name);
                if let Some(ref t) = tag
                    && tag_matches(t, "DialogTitle", ".Title") {
                        return true;
                    }
                if children_have_title(&el.children) {
                    return true;
                }
            }
            JSXChild::ExpressionContainer(container) => {
                if let Some(expr) = container.expression.as_expression()
                    && expr_has_title(expr) {
                        return true;
                    }
            }
            JSXChild::Fragment(frag) => {
                if children_have_title(&frag.children) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn expr_has_title(expr: &oxc_ast::ast::Expression) -> bool {
    match expr {
        oxc_ast::ast::Expression::JSXElement(el) => {
            let tag = jsx_opening_tag_name(&el.opening_element.name);
            if let Some(ref t) = tag
                && tag_matches(t, "DialogTitle", ".Title") {
                    return true;
                }
            children_have_title(&el.children)
        }
        oxc_ast::ast::Expression::ConditionalExpression(cond) => {
            expr_has_title(&cond.consequent) || expr_has_title(&cond.alternate)
        }
        oxc_ast::ast::Expression::ParenthesizedExpression(p) => expr_has_title(&p.expression),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["DialogContent"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::JSXElement(el) = node.kind() else {
                continue;
            };
            let Some(tag) = jsx_opening_tag_name(&el.opening_element.name) else {
                continue;
            };
            if !tag_matches(&tag, "DialogContent", ".Content") {
                continue;
            }
            if tag.contains('.') && !tag.starts_with("Dialog.") {
                continue;
            }
            if !children_have_title(&el.children) {
                let (line, column) = byte_offset_to_line_col(
                    ctx.source,
                    el.opening_element.span.start as usize,
                );
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`<DialogContent>` is missing `<DialogTitle>` — required for screen readers.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}
