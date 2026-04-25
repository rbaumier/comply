//! sql-fk-naming-convention — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_ddl, RUST_STRING_KINDS};
use crate::rules::walker::collect_nodes_of_kinds;
use super::FkViolation;

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
