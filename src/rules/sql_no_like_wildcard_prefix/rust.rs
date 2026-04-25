//! sql-no-like-wildcard-prefix — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::RUST_STRING_KINDS;
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
            if !super::has_leading_wildcard_like(text) {
                continue;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "`LIKE '%...'` forces a sequential scan — use TSVECTOR + GIN \
                 index with `@@` for full-text search."
                    .into(),
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
    fn flags_leading_wildcard() {
        let src = r###"fn f() { let q = r#"SELECT * FROM t WHERE name LIKE '%x%'"#; }"###;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_suffix() {
        let src = r###"fn f() { let q = r#"SELECT * FROM t WHERE name LIKE 'x%'"#; }"###;
        assert!(run(src).is_empty());
    }
}
