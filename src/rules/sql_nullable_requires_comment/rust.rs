//! sql-nullable-requires-comment — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::RUST_STRING_KINDS;

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
        if !crate::rules::sql_helpers::is_sql_ddl(text) {
            return;
        }
        let pos = node.start_position();
        for offset in super::nullable_lines_without_comment(text) {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1 + offset,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Nullable column has no comment explaining why NULL is allowed.".into(),
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
    fn flags_nullable_in_raw_string() {
        let src = "fn f() { let q = r#\"CREATE TABLE t (\n  deleted_at TIMESTAMP,\n)\"#; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_inline_comment() {
        let src =
            "fn f() { let q = r#\"CREATE TABLE t (\n  deleted_at TIMESTAMP, -- nullable\n)\"#; }";
        assert!(run(src).is_empty());
    }
}
