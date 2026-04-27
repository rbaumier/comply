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
            // `json` as a whole word, but NOT `jsonb` — contains_word handles
            // both ends. Also skip lines containing functional usages like
            // `to_json(`, `json_build_object(`, etc., where JSON is intentional.
            if !contains_word(&lower, "json") {
                continue;
            }
            // Skip JSON-function references — only flag column type usage.
            if line_uses_json_function(&lower) {
                continue;
            }
            // Only flag in DDL contexts (CREATE TABLE / ALTER TABLE / CAST).
            if !lower.contains("create table")
                && !lower.contains("alter table")
                && !lower.contains("alter column")
                && !lower.contains("add column")
            {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: 1,
                rule_id: "sql-jsonb-not-json".into(),
                message: "`JSON` re-parses on every read. Use `JSONB` for indexable, binary storage.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

fn line_uses_json_function(lower: &str) -> bool {
    // Functions like `to_json(`, `json_build_object(`, `row_to_json(`, etc.
    lower.contains("to_json(")
        || lower.contains("row_to_json(")
        || lower.contains("json_build")
        || lower.contains("json_agg(")
        || lower.contains("json_object(")
        || lower.contains("json_array(")
        || lower.contains("json_each(")
        || lower.contains("json_extract")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.sql"), src))
    }

    #[test]
    fn flags_json_column() {
        assert_eq!(run("CREATE TABLE t (data JSON);").len(), 1);
    }

    #[test]
    fn allows_jsonb_column() {
        assert!(run("CREATE TABLE t (data JSONB);").is_empty());
    }

    #[test]
    fn flags_json_in_alter_table() {
        assert_eq!(run("ALTER TABLE t ADD COLUMN data JSON;").len(), 1);
    }

    #[test]
    fn allows_to_json_function() {
        // SELECT to_json(...) is intentional, not a column type.
        assert!(run("SELECT to_json(row(*)) FROM users;").is_empty());
    }

    #[test]
    fn ignores_identifier_containing_json() {
        // `json_data` as a column name is fine.
        assert!(run("CREATE TABLE t (json_data TEXT);").is_empty());
    }
}
