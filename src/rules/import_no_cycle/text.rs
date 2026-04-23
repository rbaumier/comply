//! import-no-cycle backend — detect circular import dependencies.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const RULE_ID: &str = "import-no-cycle";

#[derive(Debug)]
pub struct Check;

fn find_cycle(
    index: &crate::project::ImportIndex,
    start: &Path,
    current: &Path,
    visited: &mut HashSet<PathBuf>,
    path: &mut Vec<PathBuf>,
) -> Option<Vec<PathBuf>> {
    if visited.contains(current) {
        if current == start && !path.is_empty() {
            let mut cycle = path.clone();
            cycle.push(current.to_path_buf());
            return Some(cycle);
        }
        return None;
    }

    visited.insert(current.to_path_buf());
    path.push(current.to_path_buf());

    for imp in index.get_imports(current) {
        if let Some(source) = &imp.source_path {
            if source == start && !path.is_empty() {
                let mut cycle = path.clone();
                cycle.push(source.clone());
                return Some(cycle);
            }
            if let Some(cycle) = find_cycle(index, start, source, visited, path) {
                return Some(cycle);
            }
        }
    }

    path.pop();
    None
}

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
    names.join(" → ")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let index = ctx.project.import_index();
        if index.is_empty() {
            return Vec::new();
        }

        let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());

        let mut visited = HashSet::new();
        let mut path = Vec::new();

        if let Some(cycle) = find_cycle(index, &canon, &canon, &mut visited, &mut path) {
            let formatted = format_cycle(&cycle, ctx.project.project_root.as_deref());
            return vec![Diagnostic {
                path: ctx.path.to_path_buf(),
                line: 1,
                column: 1,
                rule_id: RULE_ID.into(),
                message: format!("Circular import detected: {formatted}"),
                severity: Severity::Warning,
                span: None,
            }];
        }

        Vec::new()
    }
}
