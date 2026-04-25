//! sql-no-function-on-indexed-column — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_string, TS_STRING_KINDS};
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
            if !is_sql_string(text) {
                continue;
            }
            let Some(func) = super::find_bad_func_in_where(text) else {
                continue;
            };
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!(
                    "`{func}` in WHERE defeats the index — normalize the column or add a functional index."
                ),
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
    fn flags_date_trunc() {
        let src = r#"const q = "SELECT id FROM log WHERE date_trunc('day', created_at) = '2024-01-01'";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_lower() {
        let src = r#"const q = "SELECT id FROM user WHERE LOWER(email) = 'a@b.c'";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_plain_column_comparison() {
        let src = r#"const q = "SELECT id FROM user WHERE email = 'a@b.c'";"#;
        assert!(run(src).is_empty());
    }
}
