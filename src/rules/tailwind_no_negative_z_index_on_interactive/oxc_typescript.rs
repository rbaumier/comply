use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_interactive_tag(tag: &str) -> bool {
    matches!(tag, "button" | "a")
}

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
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        let oxc_ast::ast::JSXElementName::Identifier(tag_ident) = &opening.name else { return };
        let tag = tag_ident.name.as_str();

        let mut role_button = false;
        let mut neg_z_class: Option<String> = None;

        for attr_item in &opening.attributes {
            let oxc_ast::ast::JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let oxc_ast::ast::JSXAttributeName::Identifier(name_ident) = &attr.name else { continue };
            let name = name_ident.name.as_str();
            match name {
                "role" => {
                    if let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(s)) = &attr.value
                        && s.value.as_str() == "button" {
                            role_button = true;
                        }
                }
                "className" | "class" => {
                    if let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(s)) = &attr.value
                        && let Some(c) = s.value.as_str().split_whitespace().find(|c| c.starts_with("-z-")) {
                            neg_z_class = Some(c.to_string());
                        }
                }
                _ => {}
            }
        }

        let interactive = is_interactive_tag(tag) || role_button;
        if !interactive {
            return;
        }
        let Some(klass) = neg_z_class else { return };

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`<{tag}>` has `{klass}` \u{2014} negative z-index sends interactive elements behind their stacking context and blocks clicks."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
