//! sql-singular-table-names — oxc backend for TS / JS / TSX.

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
        for name in super::find_plural_table_names(&text) {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("Table `{name}` appears plural — use singular (one row = one entity)."),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_plural_users() {
        let src = r#"const m = "CREATE TABLE users (id INT);";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_singular() {
        let src = r#"const m = "CREATE TABLE user_account (id INT);";"#;
        assert!(run_on(src).is_empty());
    }



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }


    #[test]
    fn flags_if_not_exists_orders() {
        let src = r#"const m = "CREATE TABLE IF NOT EXISTS orders (id INT);";"#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_status_exception() {
        let src = r#"const m = "CREATE TABLE status (id INT);";"#;
        assert!(run(src).is_empty());
    }
}
