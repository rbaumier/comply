//! next-no-head-element — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXElementName;
use std::sync::Arc;

pub struct Check;

fn is_document_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("_document.")
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
        if ctx.project.framework != Framework::NextJs {
            return;
        }
        if is_document_file(ctx.path) {
            return;
        }

        let tag = match &opening.name {
            JSXElementName::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if tag != "head" {
            return;
        }

        let span_start = opening.span.start as usize;
        let span_len = (opening.span.end - opening.span.start) as usize;
        let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `next/head` (pages) or the metadata API (app) instead of a raw `<head>` element.".into(),
            severity: Severity::Warning,
            span: Some((span_start, span_len)),
        });
    }
}
