use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Scan the whole source (not line by line) so clause position is
        // resolved across newlines — a multi-line `WHERE ... LIKE '%x%'` filter
        // is still flagged, while a `SELECT`-projection column is exempt.
        super::filter_leading_wildcard_like_offsets(ctx.source)
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "sql-no-like-wildcard-prefix".into(),
                    message: "`LIKE '%...'` forces a sequential scan — use TSVECTOR + GIN index with `@@` for full-text search.".into(),
                    severity: Severity::Error,
                    span: None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags() {
        assert_eq!(run("WHERE name LIKE '%test%'").len(), 1);
    }

    #[test]
    fn allows_suffix() {
        assert!(run("WHERE name LIKE 'test%'").is_empty());
    }

    #[test]
    fn allows_projection_like_as_alias() {
        // FP #7778: an aliased computed column, not a filter predicate.
        assert!(run("codebase LIKE '%.tar' as use_tar,").is_empty());
    }

    #[test]
    fn flags_multiline_where_filter() {
        // Whole-source scan keeps clause context across newlines.
        let src = "SELECT *\nFROM t\nWHERE name LIKE '%x%'";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiline_and_chain_filter() {
        let src = "SELECT *\nFROM t\nWHERE a = 1\n  AND col LIKE '%y%'";
        assert_eq!(run(src).len(), 1);
    }
}
