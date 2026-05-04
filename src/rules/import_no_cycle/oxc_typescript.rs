//! import-no-cycle OxcCheck backend.
//!
//! Detect circular import dependencies using the precomputed import index.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct Check;

fn format_cycle(cycle: &[PathBuf], root: Option<&Path>) -> String {
    let names: Vec<&str> = cycle
        .iter()
        .map(|p| {
            if let Some(r) = root {
                p.strip_prefix(r)
                    .ok()
                    .and_then(|s| s.to_str())
                    .unwrap_or_else(|| p.file_name().and_then(|n| n.to_str()).unwrap_or("?"))
            } else {
                p.file_name().and_then(|n| n.to_str()).unwrap_or("?")
            }
        })
        .collect();
    names.join(" \u{2192} ")
}

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

        if let Some(cycle) = index.cycle_for(&canon) {
            let formatted = format_cycle(cycle, ctx.project.project_root.as_deref());
            vec![Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!("Circular import detected: {formatted}"),
                severity: Severity::Warning,
                span: None,
            }]
        } else {
            Vec::new()
        }
    }
}
