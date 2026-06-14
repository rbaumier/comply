//! sql-no-truncate-in-app — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::RUST_STRING_KINDS;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(RUST_STRING_KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.in_benchmark_dir() {
            return;
        }
        let source_bytes = ctx.source.as_bytes();
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        if !super::sql_uses_truncate(text) {
            return;
        }
        if !super::looks_like_sql_truncate(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`TRUNCATE` bypasses triggers and audit — use `DELETE FROM` instead.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
    }

    fn run_at(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, src, path)
    }

    #[test]
    fn flags_truncate_table() {
        let src = r#"fn f() { let q = "TRUNCATE TABLE users"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_delete_from() {
        let src = r#"fn f() { let q = "DELETE FROM users"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_truncate_in_benchmark_dir_issue1497() {
        // Issue #1497: benchmark cleanup uses TRUNCATE to reset state fast.
        let src = r#"
            pub const CLEANUP_QUERIES: &[&str] = &[
                "TRUNCATE TABLE comments",
                "TRUNCATE TABLE posts",
                "TRUNCATE TABLE users",
            ];
        "#;
        assert!(run_at(src, "diesel_bench/benches/consts.rs").is_empty());
    }

    #[test]
    fn allows_truncate_in_test_dir_issue1497() {
        let src = r#"fn cleanup() { let q = "TRUNCATE TABLE users"; }"#;
        assert!(run_at(src, "tests/db/cleanup.rs").is_empty());
    }

    #[test]
    fn flags_truncate_in_production_code_issue1497() {
        // Negative-space guard: ordinary production code is still flagged.
        let src = r#"fn cleanup() { let q = "TRUNCATE TABLE users"; }"#;
        assert_eq!(run_at(src, "src/db/cleanup.rs").len(), 1);
    }
}
