//! sql-singular-table-names — Rust backend.

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
            for name in super::find_plural_table_names(text) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    format!(
                        "Table `{name}` appears plural — use singular (one row = one entity)."
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
    fn flags_plural_users() {
        let src = r#"fn f() { let m = "CREATE TABLE users (id INT);"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_singular() {
        let src = r#"fn f() { let m = "CREATE TABLE user_account (id INT);"; }"#;
        assert!(run(src).is_empty());
    }
}
