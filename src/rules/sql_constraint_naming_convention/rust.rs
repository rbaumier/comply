//! sql-constraint-naming-convention — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_ddl, RUST_STRING_KINDS};
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        for node in collect_nodes_of_kinds(tree, RUST_STRING_KINDS) {
            let Ok(text) = node.utf8_text(source_bytes) else {
                continue;
            };
            if !is_sql_ddl(text) {
                continue;
            }
            for name in super::find_bad_constraint_names(text) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    format!(
                        "Constraint `{name}` must end with _pk|_fk|_key|_chk|_exl|_idx (format: {{table}}_{{col}}_{{suffix}})."
                    ),
                    Severity::Warning,
                ));
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(src, &Check)
    }

    #[test]
    fn flags_missing_suffix() {
        let src = r#"fn f() { let m = "ALTER TABLE t ADD CONSTRAINT user_email UNIQUE (email)"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_key_suffix() {
        let src = r#"fn f() { let m = "ALTER TABLE t ADD CONSTRAINT user_email_key UNIQUE (email)"; }"#;
        assert!(run(src).is_empty());
    }
}
