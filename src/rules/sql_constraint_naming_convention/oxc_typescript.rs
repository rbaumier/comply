//! sql-constraint-naming-convention — oxc backend for TS / JS / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::sql_helpers::is_sql_ddl;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (text, offset) = match node.kind() {
            AstKind::StringLiteral(lit) => (lit.value.as_str().to_string(), lit.span.start as usize),
            AstKind::TemplateLiteral(tpl) => {
                let s: String = tpl.quasis.iter().map(|q| q.value.raw.as_str()).collect::<Vec<_>>().join(" ");
                (s, tpl.span.start as usize)
            }
            _ => return,
        };
        if !is_sql_ddl(&text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        for name in super::find_bad_constraint_names(&text) {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Constraint `{name}` must end with _pk|_fk|_key|_chk|_exl|_idx|_pkey|_fkey (format: {{table}}_{{col}}_{{suffix}})."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_missing_suffix() {
        let src = r#"const m = "ALTER TABLE t ADD CONSTRAINT user_email UNIQUE (email)";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_key_suffix() {
        let src = r#"const m = "ALTER TABLE t ADD CONSTRAINT user_email_key UNIQUE (email)";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_postgres_pkey_suffix() {
        let src = r#"const m = await sql`CREATE TABLE "workflow" ("id" uuid NOT NULL, CONSTRAINT "workflow_pkey" PRIMARY KEY ("id"))`.execute(db);"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_postgres_fkey_suffix() {
        let src = r#"const m = await sql`CREATE TABLE "workflow" ("ownerId" uuid NOT NULL, CONSTRAINT "workflow_ownerId_fkey" FOREIGN KEY ("ownerId") REFERENCES "user"("id"))`.execute(db);"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_constraint_without_recognized_suffix() {
        let src = r#"const m = await sql`CREATE TABLE "workflow" ("id" uuid NOT NULL, CONSTRAINT "workflow_bad" PRIMARY KEY ("id"))`.execute(db);"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn skips_if_exists_keywords_after_drop_constraint() {
        let src = r#"const m = await sql`ALTER TABLE t DROP CONSTRAINT IF EXISTS valid_name_fkey`.execute(tx);"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn name_ending_in_constraint_does_not_rematch_next_keyword() {
        let src = r#"const m = await sql`alter table smart_search add constraint dim_size_constraint check (array_length(embedding, 1) = 64)`.execute(trx);"#;
        // `dim_size_constraint` lacks a valid suffix → exactly one finding;
        // `check` must NOT be re-extracted as a second constraint name.
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("dim_size_constraint"));
    }

    #[test]
    fn parses_following_constraint_after_name_ending_in_constraint() {
        let src = r#"const m = await sql`ALTER TABLE t ADD CONSTRAINT t_a_constraint CHECK (a > 0), ADD CONSTRAINT t_b_key UNIQUE (b)`.execute(trx);"#;
        // Only `t_a_constraint` is misnamed; `t_b_key` is valid and the scan
        // must reach it correctly after the name ending in `constraint`.
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("t_a_constraint"));
    }

    #[test]
    fn rename_constraint_does_not_flag_old_typeorm_hash_name() {
        let src = r#"const m = await sql`ALTER TABLE "user" RENAME CONSTRAINT "PK_a3ffb1c0c8416b9fc6f907b7433" TO "user_pkey"`.execute(db);"#;
        // The old TypeORM hash name is being removed; the new name is valid.
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn rename_constraint_validates_new_name() {
        let src = r#"const m = await sql`ALTER TABLE t RENAME CONSTRAINT "old_pkey" TO "BadName"`.execute(db);"#;
        // The new name `BadName` lacks a valid suffix and must be flagged;
        // the old name `old_pkey` must not appear.
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("BadName"));
        assert!(!diags[0].message.contains("old_pkey"));
    }

    #[test]
    fn add_constraint_with_bad_name_still_flagged() {
        let src = r#"const m = await sql`ALTER TABLE t ADD CONSTRAINT bad_name UNIQUE (email)`.execute(db);"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bad_name"));
    }
}
