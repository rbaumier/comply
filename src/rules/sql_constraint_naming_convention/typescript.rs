//! sql-constraint-naming-convention — TS / JS / TSX backend.

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
        for name in super::find_bad_constraint_names(text) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!(
                    "Constraint `{name}` must end with _pk|_fk|_key|_chk|_exl|_idx|_pkey|_fkey (format: {{table}}_{{col}}_{{suffix}})."
                ),
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
    fn flags_missing_suffix() {
        let src = r#"const m = "ALTER TABLE t ADD CONSTRAINT user_email UNIQUE (email)";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_key_suffix() {
        let src = r#"const m = "ALTER TABLE t ADD CONSTRAINT user_email_key UNIQUE (email)";"#;
        assert!(run(src).is_empty());
    }
}
