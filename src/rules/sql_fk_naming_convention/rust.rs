//! sql-fk-naming-convention — Rust backend.

use super::FkViolation;
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
        for v in super::find_fk_violations(text) {
            let message = match v {
                FkViolation::MissingConstraintClause => {
                    "FOREIGN KEY without CONSTRAINT clause — name it `{from_table}_{from_col}_{to_table}_{to_col}_fk`.".to_string()
                }
                FkViolation::BadShape(name) => format!(
                    "FK `{name}` must follow `{{from_table}}_{{from_col}}_{{to_table}}_{{to_col}}_fk`."
                ),
            };
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                message,
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
    }

    #[test]
    fn flags_short_name() {
        let src = r#"fn f() { let m = "ALTER TABLE t ADD CONSTRAINT user_fk FOREIGN KEY (user_id) REFERENCES users(id)"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_full_shape() {
        let src = r#"fn f() { let m = "ALTER TABLE t ADD CONSTRAINT order_user_id_user_id_fk FOREIGN KEY (user_id) REFERENCES user(id)"; }"#;
        assert!(run(src).is_empty());
    }
}
