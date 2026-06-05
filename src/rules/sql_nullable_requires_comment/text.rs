use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const SQL_TYPES: &[&str] = &[
    "INTEGER",
    "INT",
    "BIGINT",
    "SMALLINT",
    "TEXT",
    "VARCHAR",
    "CHAR",
    "BOOLEAN",
    "BOOL",
    "TIMESTAMP",
    "DATE",
    "DECIMAL",
    "NUMERIC",
    "FLOAT",
    "REAL",
    "DOUBLE",
    "UUID",
    "JSONB",
    "JSON",
    "BYTEA",
    "SERIAL",
    "BIGSERIAL",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !crate::rules::sql_helpers::is_sql_ddl(ctx.source) {
            return Vec::new();
        }
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diags = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let upper = line.to_ascii_uppercase();
            let t = upper.trim();
            if !SQL_TYPES.iter().any(|ty| t.contains(ty)) {
                continue;
            }
            if t.contains("NOT NULL") || t.contains("PRIMARY KEY") {
                continue;
            }
            if t.starts_with("CREATE") || t.starts_with("ALTER") || t.starts_with("--") {
                continue;
            }
            let prev_is_comment = i > 0 && lines[i - 1].trim().starts_with("--");
            let has_inline_comment = line.contains("--");
            if !prev_is_comment && !has_inline_comment {
                diags.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Nullable column has no comment explaining why NULL is allowed."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
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
}
