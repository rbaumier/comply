//! sql-boolean-column-prefix — `.sql` file backend.
//!
//! Splits the file on `;` so DDL detection is per-statement (`CREATE
//! TABLE` blocks span multiple lines), then runs `find_bad_boolean_columns`
//! on each DDL statement. Each offending column emits one diagnostic
//! anchored at the statement's start line.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::sql_helpers::is_sql_ddl;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut start_line: usize = 1;
        let mut buf = String::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if buf.is_empty() {
                start_line = idx + 1;
            }
            buf.push_str(line);
            buf.push('\n');
            if line.contains(';') {
                if is_sql_ddl(&buf) {
                    push_for_buf(&buf, ctx, start_line, &mut diagnostics);
                }
                buf.clear();
            }
        }
        if !buf.trim().is_empty() && is_sql_ddl(&buf) {
            push_for_buf(&buf, ctx, start_line, &mut diagnostics);
        }
        diagnostics
    }
}

fn push_for_buf(buf: &str, ctx: &CheckCtx, line: usize, out: &mut Vec<Diagnostic>) {
    for col in super::find_bad_boolean_columns(buf) {
        out.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line,
            column: 1,
            rule_id: super::META.id.into(),
            message: format!(
                "BOOLEAN column `{col}` should start with `is_` or `has_` so call sites read as predicates."
            ),
            severity: Severity::Warning,
            span: None,
        });
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
    fn flags_bare_boolean_column() {
        let src = "CREATE TABLE t (active BOOLEAN NOT NULL);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_is_prefix() {
        let src = "CREATE TABLE t (is_active BOOLEAN NOT NULL);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_has_prefix() {
        let src = "CREATE TABLE t (has_admin BOOLEAN NOT NULL);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_ddl() {
        let src = "SELECT active FROM t WHERE active = TRUE;";
        assert!(run(src).is_empty());
    }
}
