//! migration-needs-lock-timeout — TS / JS / TSX backend.
//!
//! Scoped to migration files. Walks string / template literals, uses
//! `contains_ddl` to confirm the string is a complete DDL statement
//! before checking for `lock_timeout`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["string", "template_string"] => |node, source, ctx, diagnostics|
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
        crate::rules::test_helpers::run_rule(&Check, src, "/app/migrations/001_add_col.ts")
    }

    fn run_non_migration(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
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

    #[test]
    fn skips_inline_snapshot_in_test_dir_issue3354() {
        // Issue #3354: the `migrate/src/__tests__/` path contains `migrate`,
        // so `is_migration_path` matches, but the DDL string is a
        // `toMatchInlineSnapshot` assertion capturing generated migration
        // output — never executed. The central `skip_in_test_dir` gate
        // exempts it.
        let src = r#"expect(ctx.fs.read(f)).toMatchInlineSnapshot(`
"-- CreateTable
CREATE TABLE \"Order\" (\"id\" INTEGER NOT NULL);
"
`)"#;
        assert!(
            crate::rules::test_helpers::run_rule_gated(
                &Check,
                src,
                "packages/migrate/src/__tests__/MigrateDev.test.ts",
            )
            .is_empty()
        );
    }

    #[test]
    fn still_flags_genuine_migration_file() {
        // Over-exemption guard: a real migration under `migrations/` (not a
        // test dir) must still flag.
        let src = r#"const m = "ALTER TABLE users ADD COLUMN age INT";"#;
        assert_eq!(
            crate::rules::test_helpers::run_rule_gated(
                &Check,
                src,
                "app/migrations/001_add_col.ts",
            )
            .len(),
            1
        );
    }
}
