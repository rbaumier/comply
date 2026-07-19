//! sql-no-like-wildcard-prefix — Rust backend.

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
        let source_bytes = ctx.source.as_bytes();
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        if !super::has_filter_leading_wildcard_like(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`LIKE '%...'` forces a sequential scan — use TSVECTOR + GIN \
             index with `@@` for full-text search."
                .into(),
            Severity::Error,
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

    #[test]
    fn flags_leading_wildcard() {
        let src = r###"fn f() { let q = r#"SELECT * FROM t WHERE name LIKE '%x%'"#; }"###;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_suffix() {
        let src = r###"fn f() { let q = r#"SELECT * FROM t WHERE name LIKE 'x%'"#; }"###;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_projection_like_as_alias() {
        // FP #7778: `LIKE '%...' as alias` in the SELECT projection list is a
        // per-row computed boolean column, not a row-pruning filter.
        let src = r###"fn f() { let q = r#"SELECT content, codebase LIKE '%.tar' as use_tar, codebase LIKE '%.esm%' as is_esm FROM script WHERE hash = $1 LIMIT 1"#; }"###;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_unaliased_projection_like() {
        let src = r###"fn f() { let q = r#"SELECT x LIKE '%y' FROM t"#; }"###;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_where_filter() {
        let src = r###"fn f() { let q = r#"SELECT * FROM t WHERE asset_path LIKE '%/%'"#; }"###;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_and_chain_filter() {
        let src = r###"fn f() { let q = r#"SELECT * FROM t WHERE a = 1 AND col LIKE '%x%'"#; }"###;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_join_on_filter() {
        let src = r###"fn f() { let q = r#"SELECT * FROM a JOIN b ON b.name LIKE '%x%'"#; }"###;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_having_filter() {
        let src = r###"fn f() { let q = r#"SELECT n FROM t GROUP BY n HAVING max(n) LIKE '%x%'"#; }"###;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_filter_after_subquery() {
        // A subquery's `FROM` sits between the governing `WHERE` and the LIKE;
        // parenthesis-depth tracking keeps the outer `WHERE` in scope.
        let src = r###"fn f() { let q = r#"SELECT * FROM t WHERE id IN (SELECT id FROM u) AND name LIKE '%x%'"#; }"###;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_filter_even_when_projection_like_present() {
        // The projection `LIKE ... AS f` is exempt, but the WHERE filter `LIKE`
        // in the same query must still fire.
        let src = r###"fn f() { let q = r#"SELECT c LIKE '%.tar' AS f FROM t WHERE name LIKE '%z%'"#; }"###;
        assert_eq!(run(src).len(), 1);
    }
}
