//! import-export OXC backend — flag duplicate export names within a single module.
//! This rule relies on the project-level import index, so it uses run_on_semantic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::ExportKind;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::collections::HashMap;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
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

        let exports = index.get_exports(&canon);
        let mut by_name: HashMap<&str, Vec<&crate::project::import_index::ExportedSymbol>> =
            HashMap::new();
        for exp in exports {
            if exp.kind == ExportKind::StarReExport {
                continue;
            }
            by_name.entry(exp.name.as_str()).or_default().push(exp);
        }

        let mut duplicates: Vec<(&str, usize)> = Vec::new();
        for (name, occurrences) in &by_name {
            if occurrences.len() < 2 {
                continue;
            }
            let mut lines: Vec<usize> = occurrences.iter().map(|e| e.line).collect();
            lines.sort_unstable();
            for line in lines.iter().skip(1) {
                duplicates.push((name, *line));
            }
        }

        duplicates.sort_by_key(|(_, line)| *line);

        let mut diagnostics = Vec::new();
        for (name, line) in duplicates {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!("Duplicate export `{name}` in this module."),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
