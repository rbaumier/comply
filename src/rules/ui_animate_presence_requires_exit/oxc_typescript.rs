//! OXC backend for ui-animate-presence-requires-exit.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXElementName};
use std::sync::Arc;

/// Extract full tag name from a JSX opening element (handles `motion.div` etc).
fn jsx_tag_name<'a>(opening: &'a oxc_ast::ast::JSXOpeningElement<'a>) -> Option<String> {
    match &opening.name {
        JSXElementName::Identifier(id) => Some(id.name.to_string()),
        JSXElementName::IdentifierReference(id) => Some(id.name.to_string()),
        JSXElementName::MemberExpression(member) => {
            // e.g. `motion.div` → object=motion, property=div
            Some(format!("{}.{}", member_object_name(&member.object), member.property.name))
        }
        JSXElementName::NamespacedName(ns) => {
            Some(format!("{}:{}", ns.namespace.name, ns.name.name))
        }
        _ => None,
    }
}

fn member_object_name(obj: &oxc_ast::ast::JSXMemberExpressionObject) -> String {
    match obj {
        oxc_ast::ast::JSXMemberExpressionObject::IdentifierReference(id) => id.name.to_string(),
        oxc_ast::ast::JSXMemberExpressionObject::MemberExpression(m) => {
            format!("{}.{}", member_object_name(&m.object), m.property.name)
        }
        _ => String::new(),
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["AnimatePresence"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };
        let Some(tag) = jsx_tag_name(opening) else { return };
        if !tag.starts_with("motion.") {
            return;
        }

        // Check for `exit` attribute.
        let has_exit = opening.attributes.iter().any(|attr| {
            if let JSXAttributeItem::Attribute(a) = attr {
                if let oxc_ast::ast::JSXAttributeName::Identifier(name) = &a.name {
                    return name.name.as_str() == "exit";
                }
            }
            false
        });
        if has_exit {
            return;
        }

        // Walk ancestors looking for AnimatePresence.
        let mut inside_presence = false;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if let AstKind::JSXOpeningElement(parent_opening) = ancestor.kind() {
                if let Some(parent_tag) = jsx_tag_name(parent_opening) {
                    if parent_tag == "AnimatePresence" {
                        inside_presence = true;
                        break;
                    }
                }
            }
        }
        if !inside_presence {
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
                "<{tag}> inside <AnimatePresence> is missing an `exit` prop — it will vanish without animating out."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
