use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !crate::rules::sql_helpers::is_migration_path(ctx.path) {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            if upper.contains("CONCURRENTLY") {
                continue;
            }
            if upper.contains("CREATE INDEX") || upper.contains("CREATE UNIQUE INDEX") {
                diags.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`CREATE INDEX` without `CONCURRENTLY` locks the table. Use `CREATE INDEX CONCURRENTLY` instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(
            Path::new("/app/migrations/001.sql"),
            src,
        ))
    }

    fn run_non_migration(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("/app/src/schema.sql"), src))
    }

    #[test]
    fn flags_create_index() {
        assert_eq!(run("CREATE INDEX idx_email ON users(email);").len(), 1);
    }

    #[test]
    fn flags_create_unique_index() {
        assert_eq!(
            run("CREATE UNIQUE INDEX idx_ref ON orders(reference);").len(),
            1
        );
    }

    #[test]
    fn allows_concurrently() {
        assert!(run("CREATE INDEX CONCURRENTLY idx_email ON users(email);").is_empty());
    }

    #[test]
    fn flags_in_template_literal() {
        assert_eq!(run(r#"const q = `CREATE INDEX idx_x ON t(x)`;"#).len(), 1);
    }

    #[test]
    fn skips_non_migration_path() {
        assert!(run_non_migration("CREATE INDEX idx_email ON users(email);").is_empty());
    }
}
