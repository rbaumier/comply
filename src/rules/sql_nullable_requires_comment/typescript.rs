//! sql-nullable-requires-comment — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;
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
            if !crate::rules::sql_helpers::is_sql_ddl(text) {
                continue;
            }
            let pos = node.start_position();
            for offset in super::nullable_lines_without_comment(text) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1 + offset,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message:
                        "Nullable column has no comment explaining why NULL is allowed."
                            .into(),
                    severity: Severity::Warning,
                    span: None,
                });
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
    fn flags_nullable_in_template_literal() {
        let src = "const q = `CREATE TABLE t (\n  deleted_at TIMESTAMP,\n)`;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_inline_comment() {
        let src = "const q = `CREATE TABLE t (\n  deleted_at TIMESTAMP, -- nullable until soft-delete\n)`;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_not_null() {
        let src = "const q = `CREATE TABLE t (\n  email TEXT NOT NULL,\n)`;";
        assert!(run(src).is_empty());
    }
}
