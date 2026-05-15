//! next-no-title-in-document-head oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXChild, JSXElementName};
use std::sync::Arc;

pub struct Check;

fn file_is_document(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("_document.tsx") || s.contains("_document.ts") || s.ends_with("_document.jsx")
}

fn child_is_title(child: &JSXChild) -> bool {
    let JSXChild::Element(elem) = child else {
        return false;
    };
    let name = match &elem.opening_element.name {
        JSXElementName::Identifier(id) => id.name.as_str(),
        _ => return false,
    };
    name == "title"
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["<title", "<Head"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !file_is_document(ctx.path) {
            return;
        }
        let AstKind::JSXElement(element) = node.kind() else {
            return;
        };
        let name = match &element.opening_element.name {
            JSXElementName::Identifier(id) => id.name.as_str(),
            JSXElementName::IdentifierReference(id) => id.name.as_str(),
            _ => return,
        };
        if name != "Head" {
            return;
        }
        for child in &element.children {
            if !child_is_title(child) {
                continue;
            }
            let JSXChild::Element(title_elem) = child else { continue };
            let (line, column) = byte_offset_to_line_col(
                ctx.source,
                title_elem.opening_element.span.start as usize,
            );
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Don't put `<title>` inside `_document.tsx`'s `<Head>` — \
                          it becomes a global title shared by every page. Use \
                          per-page `<Head>` from `next/head` instead."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
