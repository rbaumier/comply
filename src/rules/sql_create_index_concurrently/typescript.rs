//! sql-create-index-concurrently — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;
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
        for node in collect_nodes_of_kinds(tree, TS_STRING_KINDS) {
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
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(src, &Check, "/app/migrations/001.ts")
    }

    fn run_non_migration(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_create_index_in_template() {
        let src = r#"const q = `CREATE INDEX idx_email ON users(email)`;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_create_unique_index() {
        let src = r#"const q = "CREATE UNIQUE INDEX idx_ref ON orders(reference)";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_concurrently() {
        let src = r#"const q = "CREATE INDEX CONCURRENTLY idx_email ON users(email)";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_in_comment() {
        let src = "// CREATE INDEX idx_email ON users(email)\nconst x = 1;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_migration_path() {
        let src = r#"const q = `CREATE INDEX idx_email ON users(email)`;"#;
        assert!(run_non_migration(src).is_empty());
    }
}
