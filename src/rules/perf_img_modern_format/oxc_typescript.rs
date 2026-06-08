//! perf-img-modern-format OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeItem;
use std::sync::Arc;

fn has_legacy_extension(src: &str) -> bool {
    let lower = src.to_ascii_lowercase();
    let bare = lower.split(['?', '#']).next().unwrap_or(&lower);
    bare.ends_with(".jpg") || bare.ends_with(".jpeg") || bare.ends_with(".png")
}

fn jsx_tag_name<'a>(opening: &'a oxc_ast::ast::JSXOpeningElement<'a>) -> Option<&'a str> {
    match &opening.name {
        oxc_ast::ast::JSXElementName::Identifier(id) => Some(id.name.as_str()),
        oxc_ast::ast::JSXElementName::IdentifierReference(id) => Some(id.name.as_str()),
        _ => None,
    }
}

fn is_inside_picture(node: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::JSXOpeningElement(opening) = ancestor.kind()
            && jsx_tag_name(opening) == Some("picture") {
                return true;
            }
    }
    false
}

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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };
        if jsx_tag_name(opening) != Some("img") {
            return;
        }

        let mut src_val: Option<String> = None;
        let mut has_srcset = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let oxc_ast::ast::JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            match name.name.as_str() {
                "src" => {
                    if let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(s)) = &attr.value {
                        src_val = Some(s.value.to_string());
                    }
                }
                "srcSet" | "srcset" => {
                    has_srcset = true;
                }
                _ => {}
            }
        }

        let Some(src) = src_val else { return };
        if !has_legacy_extension(&src) {
            return;
        }
        if has_srcset {
            return;
        }
        if is_inside_picture(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`<img src=\"...jpg|.png|.jpeg\">` should offer a WebP/AVIF alternative via `<picture>` or `srcset`.".into(),
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
    fn flags_plain_jpg() {
        assert_eq!(run(r#"const x = <img src="hero.jpg" />;"#).len(), 1);
    }


    #[test]
    fn flags_plain_png() {
        assert_eq!(run(r#"const x = <img src="logo.png" alt="" />;"#).len(), 1);
    }


    #[test]
    fn allows_webp() {
        assert!(run(r#"const x = <img src="hero.webp" />;"#).is_empty());
    }


    #[test]
    fn allows_img_with_srcset() {
        assert!(
            run(r#"const x = <img src="hero.jpg" srcSet="hero.webp 1x, hero.avif 2x" />;"#)
                .is_empty()
        );
    }
}
