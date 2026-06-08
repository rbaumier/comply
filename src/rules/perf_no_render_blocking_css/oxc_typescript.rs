use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName,
};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["stylesheet"])
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
        // Must be a <link> element
        let tag_name = match &opening.name {
            JSXElementName::Identifier(ident) => ident.name.as_str(),
            _ => return,
        };
        if tag_name != "link" {
            return;
        }

        let mut rel: Option<&str> = None;
        let mut has_media = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            match name_ident.name.as_str() {
                "rel" => {
                    if let Some(JSXAttributeValue::StringLiteral(s)) = &attr.value {
                        rel = Some(s.value.as_str());
                    }
                }
                "media" => has_media = true,
                _ => {}
            }
        }

        if rel != Some("stylesheet") {
            return;
        }
        if has_media {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`<link rel=\"stylesheet\">` without a `media` attribute blocks first paint — add `media=\"...\"` so the browser can defer non-critical CSS.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_stylesheet_without_media() {
        assert_eq!(
            run(r#"const x = <link rel="stylesheet" href="/a.css" />;"#).len(),
            1
        );
    }


    #[test]
    fn allows_stylesheet_with_media() {
        assert!(
            run(r#"const x = <link rel="stylesheet" href="/a.css" media="print" />;"#).is_empty()
        );
    }


    #[test]
    fn ignores_non_stylesheet_link() {
        assert!(run(r#"const x = <link rel="preload" as="style" href="/a.css" />;"#).is_empty());
    }
}
