//! migration-needs-lock-timeout — TS / JS / TSX backend.
//!
//! Scoped to migration files. Walks string / template literals, uses
//! `is_sql_ddl` to confirm the string is DDL before checking for
//! `lock_timeout`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["string", "template_string"] => |node, source, ctx, diagnostics|
    if !crate::rules::sql_helpers::is_migration_path(ctx.path) { return; }
    let Ok(text) = node.utf8_text(source) else { return; };
    if !super::contains_ddl(text) { return; }
    if super::declares_lock_timeout(text) { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "migration-needs-lock-timeout".into(),
        message: "DDL without `SET lock_timeout` — add `SET lock_timeout = '5s';` at the top to prevent write queue pileups.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(
            src,
            &Check,
            "/app/migrations/001_add_col.ts",
        )
    }

    fn run_non_migration(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
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
    fn flags_create_index_in_template_literal() {
        let src = "const m = `CREATE INDEX idx_users_age ON users(age)`;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_non_migration_path() {
        let src = r#"const m = "ALTER TABLE users ADD COLUMN age INT";"#;
        assert!(run_non_migration(src).is_empty());
    }

    #[test]
    fn ignores_non_ddl_string() {
        let src = r#"const greeting = "hello world";"#;
        assert!(run(src).is_empty());
    }
}
