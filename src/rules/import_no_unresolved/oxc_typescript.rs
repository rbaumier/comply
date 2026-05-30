//! import-no-unresolved OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::collections::HashSet;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
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
        let base_dir = ctx.path.parent().unwrap_or(ctx.path);
        let mut seen: HashSet<(String, usize)> = HashSet::new();
        let mut diagnostics = Vec::new();

        for imp in index.get_imports(&canon) {
            let is_relative = imp.specifier.starts_with("./") || imp.specifier.starts_with("../");
            if !is_relative {
                continue;
            }
            if imp.source_path.is_some() {
                continue;
            }
            if crate::rules::path_utils::is_relative_specifier_gitignored(
                base_dir,
                &imp.specifier,
            ) {
                continue;
            }
            if !seen.insert((imp.specifier.clone(), imp.line)) {
                continue;
            }

            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: imp.line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Unable to resolve import path `{}` — file does not exist.",
                    imp.specifier
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}
