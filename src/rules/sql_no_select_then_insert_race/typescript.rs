//! sql-no-select-then-insert-race — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        // Collect every SQL-bearing string literal in source order, with its
        // node. We then look for SELECT-from-X strings followed by INSERT-into-X
        // strings without ON CONFLICT, where X matches.
        let nodes = collect_nodes_of_kinds(tree, TS_STRING_KINDS);
        let mut diagnostics = Vec::new();
        for (i, sel_node) in nodes.iter().enumerate() {
            let Ok(sel_text) = sel_node.utf8_text(source_bytes) else {
                continue;
            };
            let Some(sel_table) = super::extract_select_from_table(sel_text) else {
                continue;
            };
            for ins_node in &nodes[i + 1..] {
                let Ok(ins_text) = ins_node.utf8_text(source_bytes) else {
                    continue;
                };
                let Some(ins_table) = super::extract_insert_into_table(ins_text) else {
                    continue;
                };
                if ins_table != sel_table {
                    continue;
                }
                if super::has_on_conflict(ins_text) {
                    break;
                }
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    ins_node,
                    super::META.id,
                    format!(
                        "SELECT then INSERT on `{sel_table}` is a TOCTOU race — use `INSERT ... ON CONFLICT`."
                    ),
                    Severity::Warning,
                ));
                break;
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
    fn flags_select_then_insert_same_table() {
        let src = "const a = `SELECT id FROM user WHERE email = $1`; const b = `INSERT INTO user (email) VALUES ($1)`;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_on_conflict() {
        let src = "const a = `SELECT id FROM user WHERE email = $1`; const b = `INSERT INTO user (email) VALUES ($1) ON CONFLICT (email) DO NOTHING`;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_different_tables() {
        let src = "const a = `SELECT id FROM user`; const b = `INSERT INTO audit (x) VALUES (1)`;";
        assert!(run(src).is_empty());
    }
}
