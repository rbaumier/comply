use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    /// The justification is a comment.
    fn reads_sql_comments(&self) -> bool {
        true
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !crate::rules::sql_helpers::is_sql_ddl(ctx.source) {
            return Vec::new();
        }
        let code = crate::rules::sql_helpers::mask_sql_comments(ctx.source);
        super::nullable_lines_without_comment(ctx.source, &code)
            .into_iter()
            .map(|offset| Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: offset.saturating_add(1),
                column: 1,
                rule_id: super::META.id.into(),
                message: "Nullable column has no comment explaining why NULL is allowed.".into(),
                severity: Severity::Error,
                span: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("migration.ts"), src))
    }

    #[test]
    fn flags_nullable_without_comment() {
        assert_eq!(
            run("CREATE TABLE t (\n  deleted_at TIMESTAMP,\n);").len(),
            1
        );
    }

    #[test]
    fn allows_nullable_with_inline_comment() {
        assert!(run(
            "CREATE TABLE t (\n  deleted_at TIMESTAMP, -- null until soft-deleted\n);"
        )
        .is_empty());
    }

    #[test]
    fn allows_nullable_with_preceding_comment() {
        assert!(run(
            "CREATE TABLE t (\n  -- null until user completes profile\n  avatar_url TEXT,\n);"
        )
        .is_empty());
    }

    #[test]
    fn allows_not_null() {
        assert!(run("CREATE TABLE t (\n  email TEXT NOT NULL,\n);").is_empty());
    }

    #[test]
    fn ignores_column_in_a_block_comment() {
        assert!(run("CREATE TABLE t (\n  /* archived_at TIMESTAMP, */\n  id INT NOT NULL\n);").is_empty());
    }

    #[test]
    fn no_fp_on_vue_syntax_highlight_with_ansi_codes() {
        // Regression for #811: Vue highlighting output with ANSI escapes that
        // happen to contain SQL type keywords — not DDL, must not flag.
        let vue_content = "\x1b[33mINTEGER\x1b[0m field_name\n<template><div>TEXT content</div></template>";
        assert!(run(vue_content).is_empty());
    }

    #[test]
    fn no_fp_on_rust_snapshot_with_sql_keywords() {
        // Regression for #811: Rust test snapshot containing rule names like
        // "sql-no-varchar" or identifiers with INTEGER/TEXT — not DDL.
        let rust_snapshot = r#"assert_eq!(output, "INTEGER, TEXT, VARCHAR rules checked");"#;
        assert!(run(rust_snapshot).is_empty());
    }

    #[test]
    fn still_flags_real_ddl_nullable_column() {
        // DDL with a nullable column and no comment must still be caught.
        let ddl = "CREATE TABLE users (\n  deleted_at TIMESTAMP,\n);";
        assert_eq!(run(ddl).len(), 1);
    }

    #[test]
    fn ignores_insert_into_statements_issue_1340() {
        // Regression for #1340: `insert into` contains `INTO`, whose `INT`
        // substring previously matched the INT type. The columns are NOT
        // NULL, so the block must produce no diagnostics.
        let ddl = "create table teams (\n  id int primary key not null,\n  name text not null unique\n);\ninsert into teams (id, name) values (1, 'a');\ninsert into teams (id, name) values (2, 'b');";
        assert!(run(ddl).is_empty(), "{:?}", run(ddl));
    }

    #[test]
    fn point_geometry_not_misclassified_as_int_issue_1340() {
        // `POINT` contains `INT` as a substring; a nullable GEOMETRY/POINT
        // column is its own type, not an INT, but it must still be flagged
        // as a nullable column (via GEOMETRY's own keyword would require it
        // in SQL_TYPES; here we assert POINT alone is not matched as INT).
        let ddl = "CREATE TABLE shapes (\n  location POINT,\n);";
        // POINT is not a recognized SQL type keyword, so no diagnostic.
        assert!(run(ddl).is_empty(), "{:?}", run(ddl));
    }
}
