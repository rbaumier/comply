//! html-no-undeferred-third-party oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName,
};
use std::sync::Arc;

fn is_third_party_src(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://") || value.starts_with("//")
}

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["script"])
    }

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

        let JSXElementName::Identifier(tag_ident) = &opening.name else {
            return;
        };
        if tag_ident.name.as_str() != "script" {
            return;
        }

        let mut has_third_party_src = false;
        let mut has_defer_or_async = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            match name_ident.name.as_str() {
                "src" => {
                    if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value {
                        if is_third_party_src(lit.value.as_str()) {
                            has_third_party_src = true;
                        }
                    }
                }
                "defer" | "async" => {
                    has_defer_or_async = true;
                }
                _ => {}
            }
        }

        if has_third_party_src && !has_defer_or_async {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Third-party `<script>` without `defer` or `async` blocks HTML parsing."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
