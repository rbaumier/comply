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
                    path: ctx.path.to_path_buf(),
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
        assert_eq!(run("  deleted_at TIMESTAMP,").len(), 1);
    }

    #[test]
    fn allows_nullable_with_inline_comment() {
        assert!(run("  deleted_at TIMESTAMP, -- null until soft-deleted").is_empty());
    }

    #[test]
    fn allows_nullable_with_preceding_comment() {
        assert!(run("  -- null until user completes profile\n  avatar_url TEXT,").is_empty());
    }

    #[test]
    fn allows_not_null() {
        assert!(run("  email TEXT NOT NULL,").is_empty());
    }
}
