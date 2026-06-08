//! sql-add-constraint-not-valid — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{RUST_STRING_KINDS, is_sql_ddl};

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
        if !is_sql_ddl(text) {
            return;
        }
        if !super::sql_violates_add_constraint(text) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "ADD CONSTRAINT without NOT VALID locks the table during the scan — split into ADD ... NOT VALID + VALIDATE CONSTRAINT.".into(),
            Severity::Warning,
        ));
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
    fn flags_add_check_without_not_valid() {
        let src =
            r#"fn f() { let m = "ALTER TABLE t ADD CONSTRAINT t_age_chk CHECK (age > 0);"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_not_valid() {
        let src = r#"fn f() { let m = "ALTER TABLE t ADD CONSTRAINT t_age_chk CHECK (age > 0) NOT VALID;"; }"#;
        assert!(run(src).is_empty());
    }
}
