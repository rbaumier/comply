//! sql-no-select-then-insert-race — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::TS_STRING_KINDS;

/// Owned snapshot of one SQL-bearing string literal. Stores the text plus the
/// byte range so we can later emit a diagnostic anchored at the same span as
/// `Diagnostic::at_node` would have produced.
struct Collected {
    text: String,
    line: usize,
    column: usize,
    byte_start: usize,
    byte_len: usize,
}

#[derive(Default)]
struct State {
    nodes: Vec<Collected>,
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(TS_STRING_KINDS)
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::<State>::default())
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Ok(text) = node.utf8_text(ctx.source.as_bytes()) else {
            return;
        };
        let Some(state) = state.and_then(|s| s.downcast_mut::<State>()) else {
            return;
        };
        let pos = node.start_position();
        let range = node.byte_range();
        state.nodes.push(Collected {
            text: text.to_string(),
            line: pos.row + 1,
            column: pos.column + 1,
            byte_start: range.start,
            byte_len: range.len(),
        });
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(state) = state.and_then(|s| s.downcast::<State>().ok()) else {
            return;
        };
        let nodes = &state.nodes;
        for (i, sel) in nodes.iter().enumerate() {
            let Some(sel_table) = super::extract_select_from_table(&sel.text) else {
                continue;
            };
            for ins in &nodes[i + 1..] {
                let Some(ins_table) = super::extract_insert_into_table(&ins.text) else {
                    continue;
                };
                if ins_table != sel_table {
                    continue;
                }
                if super::has_on_conflict(&ins.text) {
                    break;
                }
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: ins.line,
                    column: ins.column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "SELECT then INSERT on `{sel_table}` is a TOCTOU race — use `INSERT ... ON CONFLICT`."
                    ),
                    severity: Severity::Warning,
                    span: Some((ins.byte_start, ins.byte_len)),
                });
                break;
            }
        }
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
