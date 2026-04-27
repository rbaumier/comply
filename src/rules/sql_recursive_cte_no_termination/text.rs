use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let upper = ctx.source.to_ascii_uppercase();

        // Walk statement-by-statement (split on `;`).
        let mut stmt_start_byte = 0usize;
        for (i, ch) in upper.char_indices() {
            if ch != ';' {
                continue;
            }
            let stmt = &upper[stmt_start_byte..i];
            if stmt.contains("WITH RECURSIVE") {
                let has_cycle = stmt.contains("CYCLE ");
                let has_depth = stmt.contains("DEPTH < ")
                    || stmt.contains("DEPTH <=")
                    || stmt.contains("LEVEL < ")
                    || stmt.contains("LEVEL <=");
                if !has_cycle && !has_depth {
                    let off = stmt.find("WITH RECURSIVE").unwrap_or(0);
                    let line = upper[..stmt_start_byte + off].matches('\n').count() + 1;
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line,
                        column: 1,
                        rule_id: "sql-recursive-cte-no-termination".into(),
                        message: "`WITH RECURSIVE` has no `CYCLE` clause or depth guard. A cycle in the data will loop forever — add `CYCLE` or `WHERE depth < N`.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            stmt_start_byte = i + 1;
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
    fn flags_recursive_without_termination() {
        let src = "WITH RECURSIVE r AS (\n  SELECT id, parent_id FROM nodes WHERE id = 1\n  UNION ALL\n  SELECT n.id, n.parent_id FROM nodes n JOIN r ON n.parent_id = r.id\n)\nSELECT * FROM r;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_recursive_with_cycle() {
        let src = "WITH RECURSIVE r AS (\n  SELECT id, parent_id, ARRAY[id] FROM nodes WHERE id = 1\n  UNION ALL\n  SELECT n.id, n.parent_id, path || n.id FROM nodes n JOIN r ON n.parent_id = r.id\n) CYCLE id SET is_cycle USING path\nSELECT * FROM r;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_recursive_with_depth_guard() {
        let src = "WITH RECURSIVE r AS (\n  SELECT id, parent_id, 0 AS depth FROM nodes WHERE id = 1\n  UNION ALL\n  SELECT n.id, n.parent_id, depth + 1 FROM nodes n JOIN r ON n.parent_id = r.id WHERE depth < 100\n)\nSELECT * FROM r;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_recursive_cte() {
        let src = "WITH r AS (SELECT * FROM nodes) SELECT * FROM r;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_lowercase() {
        let src =
            "with recursive r as (select id from t) select * from r;";
        assert_eq!(run(src).len(), 1);
    }
}
