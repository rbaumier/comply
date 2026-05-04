//! a11y-anchor-ambiguous-text oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXChild, JSXElementName};
use std::sync::Arc;

const AMBIGUOUS_TEXTS: &[&str] = &[
    "click here",
    "here",
    "link",
    "a link",
    "read more",
    "learn more",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXElement(element) = node.kind() else {
            return;
        };

        // Check the opening tag is an <a>.
        let JSXElementName::Identifier(tag_ident) = &element.opening_element.name else {
            return;
        };
        if tag_ident.name.as_str() != "a" {
            return;
        }

        // Collect text content from JSXText children.
        for child in &element.children {
            let JSXChild::Text(text) = child else {
                continue;
            };
            let trimmed = text.value.as_str().trim().to_lowercase();
            for &ambiguous in AMBIGUOUS_TEXTS {
                if trimmed == ambiguous {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, element.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Ambiguous link text \"{ambiguous}\". Use descriptive text that indicates the link's purpose."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return; // one diagnostic per element
                }
            }
        }
    }
}
