//! migration-needs-lock-timeout — OXC backend for TS / JS / TSX.
//!
//! File-level: the whole file is skipped when any of its SQL strings is
//! ClickHouse DDL — ClickHouse has no `lock_timeout` setting — so a
//! marker-less DDL string in a ClickHouse migration is not flagged alongside
//! its marked siblings. Every DDL string lacking a lock timeout is otherwise
//! flagged individually.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !crate::rules::sql_helpers::is_migration_path(ctx.path) {
            return Vec::new();
        }

        let mut any_clickhouse = false;
        let mut candidates: Vec<usize> = Vec::new();

        for node in semantic.nodes().iter() {
            let (text, offset) = match node.kind() {
                AstKind::StringLiteral(lit) => {
                    (lit.value.as_str().to_string(), lit.span.start as usize)
                }
                AstKind::TemplateLiteral(tpl) => {
                    let s: String = tpl
                        .quasis
                        .iter()
                        .map(|q| q.value.raw.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    (s, tpl.span.start as usize)
                }
                _ => continue,
            };
            if crate::rules::sql_helpers::is_clickhouse_ddl(&text) {
                any_clickhouse = true;
                continue;
            }
            if super::contains_ddl(&text) && !super::declares_lock_timeout(&text) {
                candidates.push(offset);
            }
        }

        if any_clickhouse {
            return Vec::new();
        }
        candidates
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "DDL without `SET lock_timeout` — add `SET lock_timeout = '5s';` at the top to prevent write queue pileups.".into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "db/migrations/001.ts")
    }

    #[test]
    fn flags_alter_table_without_lock_timeout() {
        let src = r#"const m = "ALTER TABLE users ADD COLUMN age INT";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_alter_table_with_lock_timeout() {
        let src = r#"const m = "SET lock_timeout = '5s'; ALTER TABLE users ADD COLUMN age INT";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_clickhouse_alter_with_type_wrapper_issue_7765() {
        let src = r#"const m = "ALTER TABLE X ADD COLUMN IF NOT EXISTS staled_at Nullable(DateTime64(6, 'UTC'))";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_marker_less_clickhouse_string_via_file_level_gate_issue_7765() {
        // The marker-less ALTER string sits alongside a MergeTree CREATE; the
        // file is classified ClickHouse, so neither string is flagged.
        let src = r#"
            const create = "CREATE TABLE Events (id UInt64) ENGINE = MergeTree ORDER BY id";
            const alter = "ALTER TABLE Events ADD COLUMN name String";
        "#;
        assert!(run(src).is_empty());
    }
}
