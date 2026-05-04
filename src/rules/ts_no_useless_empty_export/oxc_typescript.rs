//! OXC backend for ts-no-useless-empty-export.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut empty_export_spans: Vec<oxc_span::Span> = Vec::new();
        let mut has_real_export = false;

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::ExportNamedDeclaration(export) => {
                    // `export {}` has no declaration and an empty specifiers list.
                    if export.declaration.is_none()
                        && export.specifiers.is_empty()
                        && export.source.is_none()
                    {
                        empty_export_spans.push(export.span);
                    } else {
                        has_real_export = true;
                    }
                }
                AstKind::ExportDefaultDeclaration(_) => {
                    has_real_export = true;
                }
                AstKind::ImportDeclaration(_) => {
                    has_real_export = true;
                }
                _ => {}
            }
        }

        if !has_real_export {
            return Vec::new();
        }

        empty_export_spans
            .iter()
            .map(|span| {
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`export {}` is unnecessary — the file already has other exports."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}
