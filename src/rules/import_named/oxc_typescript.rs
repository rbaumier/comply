//! OxcCheck backend for import-named.
//!
//! This rule uses the project import index, not AST — same as the
//! tree-sitter version. We use `run_on_semantic` with an empty
//! `interested_kinds` since the real work is index-based.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::{ExportKind, ImportKind};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
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
        let mut diagnostics = Vec::new();
        let index = ctx.project.import_index();
        if index.is_empty() {
            return diagnostics;
        }

        let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());
        let mut exports_cache: HashMap<PathBuf, Option<HashSet<String>>> = HashMap::new();

        for imp in index.get_imports(&canon) {
            if imp.kind != ImportKind::Named {
                continue;
            }
            let Some(src) = &imp.source_path else {
                continue;
            };

            let entry = exports_cache.entry(src.clone()).or_insert_with(|| {
                let exports = index.get_exports(src);
                if exports.iter().any(|e| e.kind == ExportKind::StarReExport) {
                    return None;
                }
                Some(exports.iter().map(|e| e.name.clone()).collect())
            });

            let Some(export_names) = entry.as_ref() else {
                continue;
            };

            if !export_names.contains(&imp.imported_name) {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: imp.line,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{}` is not exported by `{}`.",
                        imp.imported_name, imp.specifier
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}
