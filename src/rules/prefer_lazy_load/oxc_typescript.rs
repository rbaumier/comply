use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXElementName};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };
        let tag = match &opening.name {
            JSXElementName::Identifier(ident) => ident.name.as_str(),
            _ => return,
        };
        if tag != "img" && tag != "iframe" {
            return;
        }

        let has_loading = opening.attributes.iter().any(|attr_item| {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                return false;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                return false;
            };
            name_ident.name.as_str() == "loading"
        });
        if has_loading {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`<{tag}>` should set `loading=\"lazy\"` to defer off-screen loads."),
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
    fn flags_img_without_loading() {
        assert_eq!(run(r#"const x = <img src="x.png" />;"#).len(), 1);
    }


    #[test]
    fn flags_iframe_without_loading() {
        assert_eq!(run(r#"const x = <iframe src="x.html" />;"#).len(), 1);
    }


    #[test]
    fn allows_img_with_lazy() {
        assert!(run(r#"const x = <img src="x.png" loading="lazy" />;"#).is_empty());
    }


    #[test]
    fn allows_img_with_eager() {
        assert!(run(r#"const x = <img src="x.png" loading="eager" />;"#).is_empty());
    }


    #[test]
    fn ignores_non_media() {
        assert!(run(r#"const x = <div />;"#).is_empty());
    }
}
