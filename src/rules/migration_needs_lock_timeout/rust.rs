//! migration-needs-lock-timeout — Rust backend.
//!
//! Scoped to migration files. Walks Rust string literals, uses
//! `is_sql_ddl` to confirm the string is DDL before checking for
//! `lock_timeout`.

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
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust_with_path(src, &Check, "/app/migrations/001.rs")
    }

    fn run_non_migration(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(src, &Check)
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
}
