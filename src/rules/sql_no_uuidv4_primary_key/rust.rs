//! sql-no-uuidv4-primary-key — Rust backend.

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
            if !super::sql_uses_uuidv4_pk(text) {
                continue;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "UUIDv4 primary keys fragment B-tree indexes — use UUIDv7 or BIGINT IDENTITY instead.".into(),
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
        crate::rules::test_helpers::run_rust(src, &Check)
    }

    #[test]
    fn flags_gen_random_uuid_on_pk() {
        let src = r#"fn f() { let q = "CREATE TABLE t (id UUID PRIMARY KEY DEFAULT gen_random_uuid())"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_non_pk_uuid() {
        let src = r#"fn f() { let q = "CREATE TABLE t (trace_id UUID DEFAULT gen_random_uuid())"; }"#;
        assert!(run(src).is_empty());
    }
}
