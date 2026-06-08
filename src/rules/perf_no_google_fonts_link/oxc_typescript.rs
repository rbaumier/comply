//! OxcCheck backend — flags JSX `<link>` whose `href` points at Google Fonts.

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
        Some(&["fonts.googleapis.com", "fonts.gstatic.com"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        // Check tag name is "link"
        let tag_name = match &opening.name {
            oxc_ast::ast::JSXElementName::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if tag_name != "link" {
            return;
        }

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let oxc_ast::ast::JSXAttributeName::Identifier(attr_name) = &attr.name else {
                continue;
            };
            if attr_name.name.as_str() != "href" {
                continue;
            }
            let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(val)) = &attr.value else {
                continue;
            };
            let href = val.value.as_str();
            if href.contains("fonts.googleapis.com") || href.contains("fonts.gstatic.com") {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, opening.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Avoid loading fonts from `fonts.googleapis.com` — self-host them to cut a third-party handshake.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_google_fonts_link() {
        let code = r#"const x = <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter" />;"#;
        assert_eq!(run(code).len(), 1);
    }


    #[test]
    fn flags_gstatic() {
        let code = r#"const x = <link rel="preconnect" href="https://fonts.gstatic.com" />;"#;
        assert_eq!(run(code).len(), 1);
    }


    #[test]
    fn allows_self_hosted_link() {
        let code = r#"const x = <link rel="stylesheet" href="/fonts/inter.css" />;"#;
        assert!(run(code).is_empty());
    }
}
