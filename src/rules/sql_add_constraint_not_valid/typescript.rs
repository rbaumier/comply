//! sql-add-constraint-not-valid — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_ddl, TS_STRING_KINDS};
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        for node in collect_nodes_of_kinds(tree, TS_STRING_KINDS) {
            let Ok(text) = node.utf8_text(source_bytes) else {
                continue;
            };
            if !is_sql_ddl(text) {
                continue;
            }
            if !super::sql_violates_add_constraint(text) {
                continue;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "ADD CONSTRAINT without NOT VALID locks the table during the scan — split into ADD ... NOT VALID + VALIDATE CONSTRAINT.".into(),
                Severity::Warning,
            ));
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_add_check_without_not_valid() {
        let src = r#"const m = "ALTER TABLE t ADD CONSTRAINT t_age_chk CHECK (age > 0);";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_add_fk_without_not_valid() {
        let src = r#"const m = "ALTER TABLE t ADD CONSTRAINT t_u_fk FOREIGN KEY (u) REFERENCES user(id);";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_not_valid() {
        let src = r#"const m = "ALTER TABLE t ADD CONSTRAINT t_age_chk CHECK (age > 0) NOT VALID;";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_comment_with_pattern() {
        let src = "// ALTER TABLE t ADD CONSTRAINT foo CHECK (x > 0)\nconst x = 1;";
        assert!(run(src).is_empty());
    }
}
