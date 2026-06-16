use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::sql_helpers::is_sql_ddl;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Float-for-money smells live in DDL column definitions. Require a
        // `CREATE TABLE` / `ALTER TABLE` (or `CREATE TYPE`) marker somewhere in
        // the file before running the per-line money/float matcher; otherwise
        // English prose in a string literal (`a total game-changer … the real
        // deal`) trips the `total` + `REAL` word match with no schema in sight.
        if !is_sql_ddl(ctx.source) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(ft) = super::float_type_for_money_line(line) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc), line: idx + 1, column: 1,
                    rule_id: super::META.id.into(),
                    message: format!("`{ft}` near a monetary column — use `NUMERIC(precision, scale)` to avoid floating-point rounding errors."),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.sql"), source))
    }

    #[test]
    fn flags() {
        assert_eq!(
            run("CREATE TABLE accounts (\n  price FLOAT NOT NULL\n);").len(),
            1
        );
    }
    #[test]
    fn allows_numeric() {
        assert!(run("CREATE TABLE accounts (\n  price NUMERIC(10, 2) NOT NULL\n);").is_empty());
    }

    #[test]
    fn flags_create_table_with_real_money_column() {
        // The rule's real purpose: a DDL column definition using a float type
        // for money still fires (matched per line).
        assert_eq!(
            run("CREATE TABLE accounts (\n  balance REAL,\n  total FLOAT\n);").len(),
            2
        );
    }

    #[test]
    fn flags_alter_table_adding_real_money_column() {
        assert_eq!(
            run("ALTER TABLE accounts ADD COLUMN price REAL").len(),
            1
        );
    }

    #[test]
    fn ignores_prose_without_ddl_issue_3289() {
        // nuxt/ui MarqueeTestimonials.vue: English prose in a Vue string
        // literal matches `total` (money word) + `real` (float type) on the
        // same line, but there is no DDL anywhere — must not fire.
        let prose = "quote: 'Wow, Nuxt UI Pro is a total game-changer! \
                     I've been able to focus on the real deal – building the app itself.'";
        assert!(Check
            .check(&CheckCtx::for_test(Path::new("Marquee.vue"), prose))
            .is_empty());
    }
}
