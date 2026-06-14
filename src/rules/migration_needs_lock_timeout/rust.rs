//! migration-needs-lock-timeout — Rust backend.
//!
//! Scoped to migration files. Walks Rust string literals, uses
//! `contains_ddl` to confirm the string is a complete DDL statement
//! before checking for `lock_timeout`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["string_literal", "raw_string_literal"] => |node, source, ctx, diagnostics|
    if !crate::rules::sql_helpers::is_migration_path(ctx.path) { return; }
    let Ok(text) = node.utf8_text(source) else { return; };
    if !super::contains_ddl(text) { return; }
    if super::declares_lock_timeout(text) { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "migration-needs-lock-timeout".into(),
        message: "DDL without `SET lock_timeout` — add `SET lock_timeout = '5s';` at the top to prevent write queue pileups.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "/app/migrations/001.rs")
    }

    fn run_non_migration(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
    }

    #[test]
    fn flags_alter_table_without_lock_timeout() {
        let src = r#"fn f() { let m = "ALTER TABLE users ADD COLUMN age INT"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_alter_table_with_lock_timeout() {
        let src = r#"fn f() { let m = "SET lock_timeout = '5s'; ALTER TABLE users ADD COLUMN age INT"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_migration_path() {
        let src = r#"fn f() { let m = "ALTER TABLE users ADD COLUMN age INT"; }"#;
        assert!(run_non_migration(src).is_empty());
    }

    #[test]
    fn ignores_non_ddl_string() {
        let src = r#"fn f() { let s = "this is just prose"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_query_builder_fragments_issue_1498() {
        // diesel diff_schema.rs: DDL assembled incrementally via a query
        // builder. Each literal is a keyword prefix with no target name and
        // no standalone statement to attach a lock_timeout to.
        let src = r#"
            fn generate_add_column(query_builder: &mut QueryBuilder, table: &str) {
                query_builder.push_sql("ALTER TABLE ");
                query_builder.push_identifier(table);
                query_builder.push_sql(" ADD COLUMN ");
                query_builder.push_sql(";");
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_complete_alter_table_statement_in_string() {
        // Negative-space guard: a genuine raw ALTER TABLE statement without a
        // lock_timeout must STILL fire.
        let src = r#"fn f() { let m = "ALTER TABLE orders ADD COLUMN total NUMERIC"; }"#;
        assert_eq!(run(src).len(), 1);
    }
}
