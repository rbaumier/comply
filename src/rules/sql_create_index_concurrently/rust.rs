//! sql-create-index-concurrently — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::RUST_STRING_KINDS;
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        if !crate::rules::sql_helpers::is_migration_path(ctx.path) {
            return vec![];
        }
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        for node in collect_nodes_of_kinds(tree, RUST_STRING_KINDS) {
            let Ok(text) = node.utf8_text(source_bytes) else {
                continue;
            };
            if !super::is_blocking_create_index(text) {
                continue;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "`CREATE INDEX` without `CONCURRENTLY` locks the table. \
                 Use `CREATE INDEX CONCURRENTLY` instead."
                    .into(),
                Severity::Warning,
            ));
        }
        diagnostics
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
        crate::rules::test_helpers::run_rule(&Check, src, "/app/migrations/001.rs")
    }

    fn run_non_migration(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
    }

    #[test]
    fn flags_create_index() {
        let src = r#"fn f() { let q = "CREATE INDEX idx_email ON users(email)"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_concurrently() {
        let src = r#"fn f() { let q = "CREATE INDEX CONCURRENTLY idx_email ON users(email)"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_migration_path() {
        let src = r#"fn f() { let q = "CREATE INDEX idx_email ON users(email)"; }"#;
        assert!(run_non_migration(src).is_empty());
    }
}
