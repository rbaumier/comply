use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.contains(".references(") && !ctx.source.contains(".index(") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(), line: idx + 1, column: 1,
                    rule_id: "drizzle-fk-needs-index".into(),
                    message: "FK `.references()` without `.index()` — PostgreSQL does NOT auto-index FK columns. Add an explicit index to avoid sequential scans on JOINs and cascading deletes.".into(),
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
    fn run(source: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("schema.ts"), source)) }

    #[test]
    fn flags_fk_without_index() { assert_eq!(run("userId: integer('user_id').references(() => users.id)").len(), 1); }
    #[test]
    fn allows_fk_with_index() { assert!(run("userId: integer('user_id').references(() => users.id)\n  .index()").is_empty()); }
}
