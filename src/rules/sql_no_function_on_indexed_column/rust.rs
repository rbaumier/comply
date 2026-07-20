//! sql-no-function-on-indexed-column — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{RUST_STRING_KINDS, is_sql_string};

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
        if !is_sql_string(text) {
            return;
        }
        let Some(func) = super::find_bad_func_in_where(text) else {
            return;
        };
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!(
                "`{func}` in WHERE defeats the index — normalize the column or add a functional index."
            ),
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
    fn flags_lower() {
        let src = r#"fn f() { let q = "SELECT id FROM user WHERE LOWER(email) = 'a'"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_plain_column_comparison() {
        let src = r#"fn f() { let q = "SELECT id FROM user WHERE email = 'a'"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_coalesce_over_subquery_on_value_side() {
        // FP #7890 (zksync-era factory_deps_dal.rs): the indexed column
        // `miniblock_number` is bare on the sargable side; COALESCE wraps a
        // subquery and a literal on the *value* side, so the index seek stands.
        let src = r##"fn f() {
    sqlx::query!(
        r#"
        SELECT bytecode
        FROM factory_deps
        WHERE
            bytecode_hash = $1
            AND miniblock_number <= COALESCE(
                (SELECT MAX(number) FROM miniblocks WHERE l1_batch_number <= $2),
                0
            )
        "#,
    );
}"##;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_coalesce_wrapping_column() {
        // COALESCE wrapping a real column still defeats the seek.
        let src = r#"fn f() { let q = "SELECT id FROM t WHERE COALESCE(deleted_at, 'inf') > now()"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_cast_of_bind_param() {
        // CAST wraps a bind parameter; the type name after `AS` is not a column.
        let src = r#"fn f() { let q = "SELECT id FROM t WHERE addr = CAST($1 AS bytea)"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_cast_of_column() {
        let src = r#"fn f() { let q = "SELECT id FROM t WHERE CAST(created_at AS date) = $1"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_extract_of_bind_param() {
        // EXTRACT's field keyword precedes FROM; the source after FROM is a param.
        let src = r#"fn f() { let q = "SELECT id FROM t WHERE y = EXTRACT(YEAR FROM $1)"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_extract_of_column() {
        let src = r#"fn f() { let q = "SELECT id FROM t WHERE EXTRACT(YEAR FROM created_at) = 2024"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_column_wrapped_in_grouping_paren() {
        // A column wrapped in a grouping / arithmetic sub-expression inside the
        // function is still wrapped: `EXTRACT(EPOCH FROM (now() - created_at))`
        // defeats the seek on `created_at`.
        let src = r#"fn f() { let q = "SELECT id FROM t WHERE EXTRACT(EPOCH FROM (now() - created_at)) > 3600"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_grouping_paren_without_column() {
        // No column anywhere in the argument list — only a function call and a
        // bind parameter inside a grouping paren, plus a literal fallback.
        let src = r#"fn f() { let q = "SELECT id FROM t WHERE ts <= COALESCE((now() - $1), now())"; }"#;
        assert!(run(src).is_empty());
    }
}
