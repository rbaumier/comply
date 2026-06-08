//! sql-singular-table-names — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{TS_STRING_KINDS, is_sql_ddl};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(TS_STRING_KINDS)
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
        if !is_sql_ddl(text) {
            return;
        }
        for name in super::find_plural_table_names(text) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!("Table `{name}` appears plural — use singular (one row = one entity)."),
                Severity::Warning,
            ));
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_plural_users() {
        let src = r#"const m = "CREATE TABLE users (id INT);";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_if_not_exists_orders() {
        let src = r#"const m = "CREATE TABLE IF NOT EXISTS orders (id INT);";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_singular() {
        let src = r#"const m = "CREATE TABLE user_account (id INT);";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_status_exception() {
        let src = r#"const m = "CREATE TABLE status (id INT);";"#;
        assert!(run(src).is_empty());
    }
}
