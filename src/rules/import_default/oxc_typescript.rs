//! import-default OxcCheck backend.
//!
//! Verify default imports target modules that actually have a default export.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::{ExportKind, ImportKind};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let index = ctx.project.import_index();
        if index.is_empty() {
            return Vec::new();
        }

        let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());
        let mut diagnostics = Vec::new();

        for imp in index.get_imports(&canon) {
            if imp.kind != ImportKind::Default {
                continue;
            }
            let Some(src) = &imp.source_path else {
                continue;
            };

            let exports = index.get_exports(src);

            // Bail if the source has `export * from '...'` — might transitively re-export a default.
            if exports.iter().any(|e| e.kind == ExportKind::StarReExport) {
                continue;
            }

            let has_default = exports
                .iter()
                .any(|e| e.kind == ExportKind::Default || e.name == "default");

            if !has_default {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: imp.line,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!("No default export found in `{}`.", imp.specifier),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}
