//! jsx-fragments OXC backend — flag `<React.Fragment>` or bare `<Fragment>`
//! opening elements, except when a `key` prop forces the long form.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXElementName};
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
            match &member.object {
                oxc_ast::ast::JSXMemberExpressionObject::IdentifierReference(id) => {
                    id.name.as_str() == "React"
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn has_key_attribute(attrs: &oxc_allocator::Vec<'_, JSXAttributeItem<'_>>) -> bool {
    attrs.iter().any(|item| {
        if let JSXAttributeItem::Attribute(attr) = item
            && let JSXAttributeName::Identifier(id) = &attr.name {
                return id.name.as_str() == "key";
            }
        false
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Fragment"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };
        if !is_fragment_tag(&opening.name) {
            return;
        }
        if has_key_attribute(&opening.attributes) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer the short fragment syntax `<>...</>`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_react_fragment() {
        let d = run_on("const x = <React.Fragment><Child /></React.Fragment>;");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_bare_fragment() {
        let d = run_on("const x = <Fragment><Child /></Fragment>;");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_short_fragment() {
        assert!(run_on("const x = <><Child /></>;").is_empty());
    }


    #[test]
    fn allows_react_fragment_with_key() {
        let src = "const x = <React.Fragment key={id}><Child /></React.Fragment>;";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_bare_fragment_with_key() {
        let src = "const x = <Fragment key={id}><Child /></Fragment>;";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_regular_component() {
        assert!(run_on("const x = <Foo><Child /></Foo>;").is_empty());
    }
}
