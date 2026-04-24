use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_migration_path(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy().to_ascii_lowercase();
        s == "migrations" || s == "migration" || s.contains("migrate")
    })
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        if !is_migration_path(ctx.path) {
            return diagnostics;
        }
        let upper = ctx.source.to_ascii_uppercase();
        let has_create_or_alter =
            upper.contains("CREATE TABLE") || upper.contains("ALTER TABLE");
        if !has_create_or_alter {
            return diagnostics;
        }
        // Accept any SET search_path = ... statement anywhere in the file.
        let compact: String = upper.chars().filter(|c| !c.is_whitespace()).collect();
        if compact.contains("SETSEARCH_PATH=") || compact.contains("SETSEARCH_PATHTO") {
            return diagnostics;
        }
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Migration must `SET search_path = pg_catalog, public;` (or use schema-qualified names) to prevent identifier hijacking.".into(),
            severity: Severity::Warning,
            span: None,
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_at(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_missing_search_path_in_migration() {
        assert_eq!(
            run_at(
                "db/migrations/001_init.sql.ts",
                "`CREATE TABLE account (id INT);`"
            )
            .len(),
            1
        );
    }

    #[test]
    fn allows_search_path_set() {
        assert!(run_at(
            "db/migrations/001_init.sql.ts",
            "`SET search_path = pg_catalog, public; CREATE TABLE account (id INT);`"
        )
        .is_empty());
    }

    #[test]
    fn ignores_non_migration_files() {
        assert!(run_at("src/repo.ts", "`CREATE TABLE account (id INT);`").is_empty());
    }
}
