//! factory-di-shape backend — flag `create*` factories with 3+ separate params.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
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
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "factory-di-shape".into(),
                    message: format!(
                        "`create*` factory with {param_count} separate params — \
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
        crate::rules::test_helpers::run_ts(source, &Check)
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
