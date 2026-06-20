//! sql-no-varchar — oxc backend for TS / JS / TSX.

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
        if !super::sql_uses_varchar_or_char(&text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`VARCHAR(N)` / `CHAR(N)` provides no perf benefit in PostgreSQL — use `TEXT` with `CHECK(length(col) <= N)`.".into(),
            severity: Severity::Error,
            span: None,
        });
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
    fn flags_varchar_in_create_table() {
        let src = r#"const m = "CREATE TABLE users (name VARCHAR(255))";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_flag_text_column() {
        let src = r#"const m = "CREATE TABLE users (name TEXT)";"#;
        assert!(run_on(src).is_empty());
    }

    /// Regression for #4936: a DB-driver integration test that deliberately
    /// uses `varchar` to exercise the type OID / field metadata must not be
    /// flagged. The same DDL in a production source file is still flagged.
    #[test]
    fn does_not_flag_varchar_in_integration_test_file() {
        let src = r#"client.query('CREATE TEMP TABLE zugzug(name varchar(10))', cb)"#;
        let in_test = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "packages/pg/test/integration/client/result-metadata-tests.js",
        );
        assert!(in_test.is_empty(), "varchar in an integration test must not be flagged");

        let in_src = crate::rules::test_helpers::run_rule_gated(&Check, src, "packages/pg/src/schema.js");
        assert_eq!(in_src.len(), 1, "varchar in production source is still flagged");
    }
}
