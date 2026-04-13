use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Only check files that look like migrations (contain "up" function).
        let has_up = ctx.source.contains("async up(") || ctx.source.contains("export async function up")
            || ctx.source.contains("exports.up") || ctx.source.contains("fn up(");
        if !has_up { return Vec::new(); }
        let has_down = ctx.source.contains("async down(") || ctx.source.contains("export async function down")
            || ctx.source.contains("exports.down") || ctx.source.contains("fn down(")
            || ctx.source.contains("rollback");
        if has_down { return Vec::new(); }
        vec![Diagnostic {
            path: ctx.path.to_path_buf(), line: 1, column: 1,
            rule_id: "migration-needs-rollback".into(),
            message: "Migration has `up()` but no `down()` / rollback — every migration must be reversible for quick recovery from bad deploys.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("migration.ts"), source)) }

    #[test]
    fn flags_no_down() { assert_eq!(run("export async function up(db) { db.exec('CREATE TABLE t (id INT)'); }").len(), 1); }
    #[test]
    fn allows_with_down() { assert!(run("export async function up(db) {} export async function down(db) {}").is_empty()); }
    #[test]
    fn ignores_non_migration() { assert!(run("function doStuff() {}").is_empty()); }
}
