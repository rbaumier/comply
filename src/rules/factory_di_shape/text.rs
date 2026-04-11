use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();

            // Match: `export function create...(`
            if !trimmed.contains("export") || !trimmed.contains("function create") {
                continue;
            }

            // Extract params between ( and ).
            let open = match trimmed.find('(') {
                Some(p) => p,
                None => continue,
            };
            let close = match trimmed[open..].find(')') {
                Some(p) => open + p,
                None => continue,
            };

            let params_str = &trimmed[open + 1..close];
            // Skip if already using destructured object pattern `{ ... }`.
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
                        "`create*` factory with {param_count} separate params — use a single deps object: `createService({{ db, cache, logger }})`."
                    ),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_create_with_many_params() {
        let src = "export function createService(db: DB, cache: Cache, logger: Logger) {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_create_with_deps_object() {
        let src = "export function createService({ db, cache, logger }: Deps) {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_create_with_two_params() {
        let src = "export function createService(db: DB, logger: Logger) {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_exported_create() {
        let src = "function createHelper(a: A, b: B, c: C) {}";
        assert!(run(src).is_empty());
    }
}
