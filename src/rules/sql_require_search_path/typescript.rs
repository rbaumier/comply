//! sql-require-search-path — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;

#[derive(Default)]
struct State {
    any_sets_search_path: bool,
    /// Location of the first CREATE/ALTER TABLE string seen, for the
    /// diagnostic emitted in `finish` when no search_path is set.
    first_ddl: Option<(usize, usize, usize, usize)>, // line, col, byte_start, byte_len
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(TS_STRING_KINDS)
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::<State>::default())
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !super::is_migration_path(ctx.path) {
            return;
        }
        let Ok(text) = node.utf8_text(ctx.source.as_bytes()) else {
            return;
        };
        let Some(state) = state.and_then(|s| s.downcast_mut::<State>()) else {
            return;
        };
        if super::sql_sets_search_path(text) {
            state.any_sets_search_path = true;
            return;
        }
        if state.first_ddl.is_none() && super::sql_creates_or_alters_table(text) {
            let pos = node.start_position();
            let range = node.byte_range();
            state.first_ddl = Some((pos.row + 1, pos.column + 1, range.start, range.len()));
        }
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !super::is_migration_path(ctx.path) {
            return;
        }
        let Some(state) = state.and_then(|s| s.downcast::<State>().ok()) else {
            return;
        };
        if state.any_sets_search_path {
            return;
        }
        let Some((line, column, byte_start, byte_len)) = state.first_ddl else {
            return;
        };
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Migration must `SET search_path = pg_catalog, public;` (or use schema-qualified names) to prevent identifier hijacking.".into(),
            severity: Severity::Warning,
            span: Some((byte_start, byte_len)),
        });
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
