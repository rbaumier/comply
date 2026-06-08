//! OXC backend for html-require-img-alt.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeItem;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["img"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        // Tag name must be `img`.
        let oxc_ast::ast::JSXElementName::Identifier(tag) = &opening.name else { return };
        if tag.name.as_str() != "img" {
            return;
        }

        // Check for `alt` attribute.
        let has_alt = opening.attributes.iter().any(|attr_item| {
            let JSXAttributeItem::Attribute(attr) = attr_item else { return false };
            let oxc_ast::ast::JSXAttributeName::Identifier(name) = &attr.name else {
                return false;
            };
            name.name.as_str() == "alt"
        });
        if has_alt {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`<img>` is missing an `alt` attribute.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }


    #[test]
    fn flags_img_without_alt() {
        assert_eq!(run(r#"const x = <img src="x.png" />;"#).len(), 1);
    }


    #[test]
    fn allows_img_with_alt() {
        assert!(run(r#"const x = <img src="x.png" alt="logo" />;"#).is_empty());
    }


    #[test]
    fn allows_empty_alt_for_decorative() {
        assert!(run(r#"const x = <img src="x.png" alt="" />;"#).is_empty());
    }


    #[test]
    fn ignores_non_img() {
        assert!(run(r#"const x = <div />;"#).is_empty());
    }
}
