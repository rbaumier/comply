//! sql-no-truncate-in-app — SQL text backend for .sql files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if ctx.file.in_benchmark_dir() {
            return vec![];
        }
        if !super::sql_uses_truncate(ctx.source) {
            return vec![];
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "`TRUNCATE` bypasses triggers, FK checks and audit — use `DELETE FROM table` instead.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.sql"), src))
    }

    #[test]
    fn flags_truncate() {
        assert_eq!(run("TRUNCATE TABLE users;").len(), 1);
    }

    #[test]
    fn allows_delete() {
        assert!(run("DELETE FROM users;").is_empty());
    }

    #[test]
    fn allows_truncate_in_benchmark_file_issue1497() {
        let path = Path::new("benches/reset.sql");
        let file = crate::rules::file_ctx::FileCtx::build(
            path,
            "TRUNCATE TABLE users;",
            crate::files::Language::Sql,
            crate::project::default_static_project_ctx(),
        );
        let ctx = CheckCtx::for_test_with_file(path, "TRUNCATE TABLE users;", &file);
        assert!(Check.check(&ctx).is_empty());
    }
}
