//! shadcn-avatar-requires-fallback OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXChild, JSXElementName};
use std::sync::Arc;

pub struct Check;

fn jsx_tag_name<'a>(name: &'a JSXElementName<'a>) -> Option<String> {
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

fn is_avatar_root(tag: &str) -> bool {
    tag == "Avatar" || tag == "Avatar.Root"
}

fn is_avatar_fallback(tag: &str) -> bool {
    tag == "AvatarFallback" || tag == "Avatar.Fallback"
}

fn children_have_fallback(children: &oxc_allocator::Vec<'_, JSXChild<'_>>) -> bool {
    for child in children.iter() {
        match child {
            JSXChild::Element(el) => {
                if let Some(ref t) = jsx_tag_name(&el.opening_element.name) {
                    if is_avatar_fallback(t) {
                        return true;
                    }
                }
                if children_have_fallback(&el.children) {
                    return true;
                }
            }
            JSXChild::ExpressionContainer(container) => {
                if let Some(expr) = container.expression.as_expression() {
                    if expr_has_fallback(expr) {
                        return true;
                    }
                }
            }
            JSXChild::Fragment(frag) => {
                if children_have_fallback(&frag.children) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn expr_has_fallback(expr: &oxc_ast::ast::Expression) -> bool {
    match expr {
        oxc_ast::ast::Expression::JSXElement(el) => {
            if let Some(ref t) = jsx_tag_name(&el.opening_element.name) {
                if is_avatar_fallback(t) {
                    return true;
                }
            }
            children_have_fallback(&el.children)
        }
        oxc_ast::ast::Expression::ConditionalExpression(cond) => {
            expr_has_fallback(&cond.consequent) || expr_has_fallback(&cond.alternate)
        }
        oxc_ast::ast::Expression::ParenthesizedExpression(p) => expr_has_fallback(&p.expression),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Avatar"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::JSXElement(el) = node.kind() else { continue };
            let Some(tag) = jsx_tag_name(&el.opening_element.name) else { continue };
            if !is_avatar_root(&tag) {
                continue;
            }
            if !children_have_fallback(&el.children) {
                let (line, column) = byte_offset_to_line_col(
                    ctx.source,
                    el.opening_element.span.start as usize,
                );
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`<Avatar>` is missing `<AvatarFallback>` \u{2014} add one so broken images still render gracefully.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}
