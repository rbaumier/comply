//! sql-require-search-path — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        if !super::is_migration_path(ctx.path) {
            return Vec::new();
        }
        let source_bytes = ctx.source.as_bytes();
        let nodes = collect_nodes_of_kinds(tree, TS_STRING_KINDS);

        // First pass: does any string set search_path?
        let mut any_sets_search_path = false;
        for n in &nodes {
            if let Ok(text) = n.utf8_text(source_bytes)
                && super::sql_sets_search_path(text)
            {
                any_sets_search_path = true;
                break;
            }
        }
        if any_sets_search_path {
            return Vec::new();
        }

        // Otherwise, flag the first SQL string containing CREATE/ALTER TABLE.
        for n in &nodes {
            let Ok(text) = n.utf8_text(source_bytes) else {
                continue;
            };
            if !super::sql_creates_or_alters_table(text) {
                continue;
            }
            return vec![Diagnostic::at_node(
                ctx.path,
                n,
                super::META.id,
                "Migration must `SET search_path = pg_catalog, public;` (or use schema-qualified names) to prevent identifier hijacking.".into(),
                Severity::Warning,
            )];
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::CheckCtx;
    use std::path::Path;

    fn run_at(path: &str, src: &str) -> Vec<Diagnostic> {
        // Parse with TS so SQL strings appear as `string`/`template_string`.
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        let path = Path::new(path);
        let ctx = CheckCtx::for_test(path, src);
        Check.check(&ctx, &tree)
    }

    #[test]
    fn flags_missing_search_path_in_migration() {
        let src = r#"const m = "CREATE TABLE account (id INT);";"#;
        assert_eq!(run_at("db/migrations/001_init.sql.ts", src).len(), 1);
    }

    #[test]
    fn allows_search_path_set() {
        let src = r#"const m = "SET search_path = pg_catalog, public; CREATE TABLE account (id INT);";"#;
        assert!(run_at("db/migrations/001_init.sql.ts", src).is_empty());
    }

    #[test]
    fn allows_search_path_in_separate_string() {
        let src = r#"const a = "SET search_path = pg_catalog, public;"; const b = "CREATE TABLE t (id INT);";"#;
        assert!(run_at("db/migrations/001_init.sql.ts", src).is_empty());
    }

    #[test]
    fn ignores_non_migration_files() {
        let src = r#"const m = "CREATE TABLE account (id INT);";"#;
        assert!(run_at("src/repo.ts", src).is_empty());
    }
}
