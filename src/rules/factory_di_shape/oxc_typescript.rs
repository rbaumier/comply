//! factory-di-shape — oxc backend.
//!
//! The original rule was text-based (line scanning). The oxc backend uses
//! `run_on_semantic` with the same line-scanning approach since this rule
//! doesn't match a specific AST node type — it matches exported function
//! declarations by line text.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();

            if !trimmed.contains("export") || !trimmed.contains("function create") {
                continue;
            }

            let open = match trimmed.find('(') {
                Some(p) => p,
                None => continue,
            };
            let close = match trimmed[open..].find(')') {
                Some(p) => open + p,
                None => continue,
            };

            let params_str = &trimmed[open + 1..close];
            if params_str.trim().starts_with('{') {
                continue;
            }

            let param_count = params_str
                .split(',')
                .filter(|p| !p.trim().is_empty())
                .count();

            if param_count >= 3 {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`create*` factory with {param_count} separate params \u{2014} \
                         use a single deps object: \
                         `createService({{ db, cache, logger }})`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_create_with_many_params() {
        let src = "export function createService(db: DB, cache: Cache, logger: Logger) {}";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_create_with_deps_object() {
        let src = "export function createService({ db, cache, logger }: Deps) {}";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_create_with_two_params() {
        let src = "export function createService(db: DB, logger: Logger) {}";
        assert!(run_on(src).is_empty());
    }
}
