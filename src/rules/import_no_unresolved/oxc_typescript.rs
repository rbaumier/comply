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
            // Skip gitignored build-time generated files (e.g. TanStack
            // Router's `routeTree.gen.ts`): often absent at lint time, always
            // present at build/dev time.
            if is_generated_specifier(&imp.specifier) {
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

/// True for specifiers pointing at a build-time generated file whose final
/// segment ends in `.gen` (e.g. `./routeTree.gen`) or carries a `.gen.`
/// extension stem (e.g. `./routeTree.gen.ts`). Such files are gitignored and
/// often absent at lint time, yet always present at build/dev time.
fn is_generated_specifier(spec: &str) -> bool {
    let last = spec.rsplit('/').next().unwrap_or(spec);
    last.ends_with(".gen") || last.contains(".gen.")
}

#[cfg(test)]
mod oxc_tests {
    use super::is_generated_specifier;

    #[test]
    fn detects_generated_specifiers_issue_487() {
        assert!(is_generated_specifier("./routeTree.gen"));
        assert!(is_generated_specifier("./routeTree.gen.ts"));
        assert!(is_generated_specifier("../app/routeTree.gen"));
        assert!(!is_generated_specifier("./routeTree"));
        assert!(!is_generated_specifier("./generated"));
    }
}
