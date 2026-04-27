use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::sql_helpers::contains_word;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            if contains_word(&lower, "serial")
                || contains_word(&lower, "bigserial")
                || contains_word(&lower, "smallserial")
            {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "sql-no-serial-use-identity".into(),
                    message: "`SERIAL`/`BIGSERIAL`/`SMALLSERIAL` create implicit sequences with awkward ownership. Use `GENERATED ALWAYS AS IDENTITY` instead.".into(),
                    severity: Severity::Warning,
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

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.sql"), src))
    }

    #[test]
    fn flags_serial() {
        assert_eq!(run("CREATE TABLE t (id SERIAL PRIMARY KEY);").len(), 1);
    }

    #[test]
    fn flags_bigserial() {
        assert_eq!(run("CREATE TABLE t (id BIGSERIAL PRIMARY KEY);").len(), 1);
    }

    #[test]
    fn flags_smallserial() {
        assert_eq!(run("CREATE TABLE t (id SMALLSERIAL PRIMARY KEY);").len(), 1);
    }

    #[test]
    fn allows_identity() {
        assert!(
            run("CREATE TABLE t (id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY);").is_empty()
        );
    }

    #[test]
    fn allows_identifier_containing_serial() {
        // `serial_number` is a column name, not the SERIAL type.
        assert!(run("CREATE TABLE t (serial_number TEXT);").is_empty());
    }
}
