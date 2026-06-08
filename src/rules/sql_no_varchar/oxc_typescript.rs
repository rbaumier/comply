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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }


    #[test]
    fn flags_char_in_alter_table() {
        let src = r#"const m = "ALTER TABLE users ADD COLUMN code CHAR(3)";"#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn does_not_flag_test_function_with_char_suffix() {
        // The user's reported FP — function name has `_char(`, not a SQL keyword.
        let src = r"function flags_negative_lookahead_same_char() { return 1; }";
        assert!(run(src).is_empty());
    }


    #[test]
    fn does_not_flag_dml_query() {
        // SELECT containing `varchar` as a CAST target — but it's DML, so
        // is_sql_ddl rejects the string before VARCHAR detection runs.
        let src = r#"const q = "SELECT CAST(x AS VARCHAR(255)) FROM t WHERE id = 1";"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn does_not_flag_comment_with_pattern() {
        let src = "// CREATE TABLE users (name VARCHAR(255))\nconst x = 1;";
        assert!(run(src).is_empty());
    }
}
