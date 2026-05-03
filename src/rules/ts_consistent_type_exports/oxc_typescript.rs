//! ts-consistent-type-exports oxc backend — flag `export { type A, type B }`
//! where every specifier uses inline `type`; prefer `export type { A, B }`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportNamedDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ExportNamedDeclaration(export) = node.kind() else { return };

        // Already a top-level `export type { ... }` — fine.
        if export.export_kind.is_type() {
            return;
        }

        // Must have specifiers (named export, not `export const ...`).
        let specs = &export.specifiers;
        if specs.is_empty() {
            return;
        }

        // Check if ALL specifiers use inline `type`.
        if !specs.iter().all(|s| s.export_kind.is_type()) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, export.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "All exported specifiers are types — use `export type { ... }` \
                      at the top level instead of inline `type` markers."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
