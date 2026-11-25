//! sql-no-truncate-in-app — SQL text backend for .sql files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::path_utils::{is_in_benchmark_directory, is_in_test_directory};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // SQL has no test file names, so only directories count.
        if is_in_benchmark_directory(ctx.path) || is_in_test_directory(ctx.path) {
            return vec![];
        }
        super::truncate_statements(ctx.source)
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`TRUNCATE` bypasses triggers, FK checks and audit — use `DELETE FROM table` instead.".into(),
                    severity: Severity::Error,
                    span: None,
                }
            })
            .collect()
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

    fn run_at(path: &str) -> Vec<Diagnostic> {
        let path = Path::new(path);
        let file = crate::rules::file_ctx::FileCtx::build(
            path,
            "TRUNCATE TABLE users;",
            crate::files::Language::Sql,
            crate::project::default_static_project_ctx(),
        );
        Check.check(&CheckCtx::for_test_with_file(
            path,
            "TRUNCATE TABLE users;",
            &file,
        ))
    }

    #[test]
    fn allows_truncate_in_benchmark_file_issue1497() {
        assert!(run_at("benches/reset.sql").is_empty());
    }

    #[test]
    fn allows_truncate_in_test_dir() {
        assert!(run_at("tests/fixtures/reset.sql").is_empty());
        assert_eq!(run_at("migrations/003.sql").len(), 1);
    }

    #[test]
    fn flags_truncate_in_a_migration_named_after_tests_or_benchmarks() {
        for name in ["V7__reset_speed_test", "V8__cpu_bench", "V9__ui.spec", "V10__seed.test"] {
            assert_eq!(run_at(&format!("migrations/{name}.sql")).len(), 1, "{name}");
        }
    }
}
