//! migration-needs-lock-timeout — OXC backend for TS / JS / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
        if !crate::rules::sql_helpers::is_migration_path(ctx.path) {
            return;
        }
        let (text, offset) = match node.kind() {
            AstKind::StringLiteral(lit) => (lit.value.as_str().to_string(), lit.span.start as usize),
            AstKind::TemplateLiteral(tpl) => {
                let s: String = tpl.quasis.iter().map(|q| q.value.raw.as_str()).collect::<Vec<_>>().join(" ");
                (s, tpl.span.start as usize)
            }
            _ => return,
        };
        if !super::contains_ddl(&text) {
            return;
        }
        if super::declares_lock_timeout(&text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "DDL without `SET lock_timeout` — add `SET lock_timeout = '5s';` at the top to prevent write queue pileups.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(src, &Check, "/app/migrations/001_add_col.ts")
    }


    fn run_non_migration(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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
