//! sql-create-index-concurrently — oxc backend for TS / JS / TSX.

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
        if !super::is_blocking_create_index(&text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`CREATE INDEX` without `CONCURRENTLY` locks the table. \
                      Use `CREATE INDEX CONCURRENTLY` instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, "/app/migrations/001.ts")
    }

    #[test]
    fn flags_create_index_in_migration() {
        let src = r#"const q = `CREATE INDEX idx_email ON users(email)`;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_concurrently() {
        let src = r#"const q = "CREATE INDEX CONCURRENTLY idx_email ON users(email)";"#;
        assert!(run_on(src).is_empty());
    }



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(src, &Check, "/app/migrations/001.ts")
    }


    fn run_non_migration(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }


    #[test]
    fn flags_create_index_in_template() {
        let src = r#"const q = `CREATE INDEX idx_email ON users(email)`;"#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_create_unique_index() {
        let src = r#"const q = "CREATE UNIQUE INDEX idx_ref ON orders(reference)";"#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn does_not_flag_in_comment() {
        let src = "// CREATE INDEX idx_email ON users(email)\nconst x = 1;";
        assert!(run(src).is_empty());
    }


    #[test]
    fn skips_non_migration_path() {
        let src = r#"const q = `CREATE INDEX idx_email ON users(email)`;"#;
        assert!(run_non_migration(src).is_empty());
    }
}
