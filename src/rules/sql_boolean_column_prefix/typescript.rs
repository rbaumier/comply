//! sql-boolean-column-prefix — TS / JS / TSX backend.

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
            for col in super::find_bad_boolean_columns(text) {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    format!(
                        "BOOLEAN column `{col}` should start with `is_` or `has_` so call sites read as predicates."
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
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_bare_boolean() {
        let src = r#"const m = "CREATE TABLE t (active BOOLEAN NOT NULL)";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_is_prefix() {
        let src = r#"const m = "CREATE TABLE t (is_active BOOLEAN NOT NULL)";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_has_prefix() {
        let src = r#"const m = "CREATE TABLE t (has_admin BOOLEAN NOT NULL)";"#;
        assert!(run(src).is_empty());
    }
}
