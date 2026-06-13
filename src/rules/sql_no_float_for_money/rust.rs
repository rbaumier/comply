//! sql-no-float-for-money — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{RUST_STRING_KINDS, is_sql_ddl, is_sql_string};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(RUST_STRING_KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        // Only treat embedded literals as money-column smells when the whole
        // literal is actually SQL. A schema dump or prose pairing a money word
        // with `double`/`float` is not SQL and must not fire (issue #1118).
        if !is_sql_string(text) && !is_sql_ddl(text) {
            return;
        }
        for line in text.lines() {
            if let Some(ft) = super::float_type_for_money_line(line) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    format!(
                        "`{ft}` near a monetary column — use `NUMERIC(precision, scale)` \
                         to avoid floating-point rounding errors."
                    ),
                    Severity::Error,
                ));
                break;
            }
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
    }

    #[test]
    fn flags_float_for_price() {
        let src = r#"fn f() { let s = "CREATE TABLE x (price FLOAT NOT NULL)"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_numeric() {
        let src = r#"fn f() { let s = "CREATE TABLE x (price NUMERIC(10, 2))"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_parquet_schema_issue_1118() {
        // Non-SQL schema dump: money word ("total") + float type ("double"),
        // but no SQL DML/DDL — must not fire.
        let src = r#"fn f() { let s = "ID: int64\nAMT_INCOME_TOTAL: double\n"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_dml_money_float() {
        let src = r#"fn f() { let s = "SELECT amount FROM ledger WHERE amount::FLOAT > 0"; }"#;
        assert_eq!(run(src).len(), 1);
    }
}
