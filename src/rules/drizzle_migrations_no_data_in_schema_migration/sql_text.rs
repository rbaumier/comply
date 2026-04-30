//! drizzle-migrations-no-data-in-schema-migration — flag .sql migrations
//! whose body contains both DDL (`CREATE TABLE` / `ALTER TABLE` / `DROP TABLE`)
//! and DML (`INSERT INTO` / `UPDATE ` / `DELETE FROM`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

fn is_migration_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("/migrations/") || s.contains("/drizzle/") || s.ends_with(".sql")
}

fn line_has_ddl(upper: &str) -> bool {
    upper.contains("CREATE TABLE")
        || upper.contains("ALTER TABLE")
        || upper.contains("DROP TABLE")
        || upper.contains("CREATE INDEX")
        || upper.contains("DROP INDEX")
}

fn line_has_dml(upper: &str) -> bool {
    upper.contains("INSERT INTO") || upper.contains("UPDATE ") || upper.contains("DELETE FROM")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_migration_file(ctx.path) {
            return Vec::new();
        }
        let mut ddl_line: Option<usize> = None;
        let mut dml_line: Option<usize> = None;
        for (idx, line) in ctx.source.lines().enumerate() {
            // Strip line comments to avoid hits on documentation.
            let stripped = match line.find("--") {
                Some(pos) => &line[..pos],
                None => line,
            };
            let upper = stripped.to_ascii_uppercase();
            if ddl_line.is_none() && line_has_ddl(&upper) {
                ddl_line = Some(idx);
            }
            if dml_line.is_none() && line_has_dml(&upper) {
                dml_line = Some(idx);
            }
            if ddl_line.is_some() && dml_line.is_some() {
                break;
            }
        }
        let (Some(_), Some(dml)) = (ddl_line, dml_line) else {
            return Vec::new();
        };
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: dml + 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Migration mixes DDL (CREATE/ALTER/DROP TABLE) with DML (INSERT/UPDATE/DELETE) — split schema and data changes into separate migrations.".to_string(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_at(src: &str, fake: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(fake), src))
    }

    #[test]
    fn flags_create_and_insert() {
        let src = "CREATE TABLE users (id INT);\nINSERT INTO users (id) VALUES (1);";
        assert_eq!(run_at(src, "drizzle/0001_init.sql").len(), 1);
    }

    #[test]
    fn flags_alter_and_update() {
        let src = "ALTER TABLE users ADD COLUMN role TEXT;\nUPDATE users SET role = 'user';";
        assert_eq!(run_at(src, "migrations/0002.sql").len(), 1);
    }

    #[test]
    fn allows_pure_schema_migration() {
        let src = "CREATE TABLE users (id INT);\nALTER TABLE users ADD COLUMN email TEXT;";
        assert!(run_at(src, "drizzle/0001.sql").is_empty());
    }

    #[test]
    fn allows_pure_data_migration() {
        let src =
            "INSERT INTO users (id) VALUES (1);\nUPDATE users SET role = 'admin' WHERE id = 1;";
        assert!(run_at(src, "drizzle/0003_seed.sql").is_empty());
    }

    #[test]
    fn ignores_comments_only() {
        let src = "-- CREATE TABLE users (id INT);\nINSERT INTO users (id) VALUES (1);";
        assert!(run_at(src, "drizzle/0001.sql").is_empty());
    }
}
