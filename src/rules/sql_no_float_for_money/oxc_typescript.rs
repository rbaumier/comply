//! sql-no-float-for-money — oxc backend for TS / JS / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::sql_helpers::{is_sql_ddl, is_sql_string};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["FLOAT", "DOUBLE", "REAL"])
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
        // Only treat embedded literals as money-column smells when the whole
        // literal is actually SQL. A Parquet/Avro/Protobuf schema or prose that
        // happens to pair a money word with `double`/`float` is not SQL and must
        // not fire (issue #1118).
        if !is_sql_string(&text) && !is_sql_ddl(&text) {
            return;
        }
        for line in text.lines() {
            if let Some(ft) = super::float_type_for_money_line(line) {
                let (line_num, column) = byte_offset_to_line_col(ctx.source, offset);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: line_num,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{ft}` near a monetary column — use `NUMERIC(precision, scale)` \
                         to avoid floating-point rounding errors."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
                // One diagnostic per node is enough.
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
    fn flags_float_for_price() {
        let src = r#"const sql = "CREATE TABLE x (price FLOAT NOT NULL)";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn does_not_flag_non_money_float() {
        let src = r#"const sql = "CREATE TABLE x (latitude FLOAT)";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_parquet_schema_issue_1118() {
        // A Parquet schema embedded in a TS fixture: `AMT_INCOME_TOTAL: double`
        // has a money word ("total") + a float type ("double"), but the string
        // is not SQL — no SELECT/FROM/WHERE, no CREATE TABLE/TYPE.
        let src = r#"const schema = `
ID: int64
CODE_GENDER: string
AMT_INCOME_TOTAL: double
NAME_INCOME_TYPE: string
`;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_ddl_money_float() {
        let src = r#"const sql = "CREATE TABLE accounts (balance DOUBLE PRECISION)";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_dml_money_float() {
        let src = r#"const sql = "SELECT amount FROM ledger WHERE amount::FLOAT > 0";"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
