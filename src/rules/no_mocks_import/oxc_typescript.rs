//! no-mocks-import oxc backend — flag imports that reference a `__mocks__` directory.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else { return };
        let spec = import.source.value.as_str();
        if !spec.contains("__mocks__") {
            return;
        }
        let raw = &ctx.source[import.source.span.start as usize..import.source.span.end as usize];
        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Import from {raw} references `__mocks__`. Let Jest/Vitest auto-resolve mocks, don't import from __mocks__ directly."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
