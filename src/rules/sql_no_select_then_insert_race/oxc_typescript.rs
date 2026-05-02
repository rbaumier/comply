//! sql-no-select-then-insert-race — oxc backend for TS / JS / TSX.
//!
//! Collects all string/template literals in a file, then checks for
//! SELECT+INSERT on the same table without ON CONFLICT.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        // We use run_on_semantic for cross-node analysis.
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut collected: Vec<(String, usize)> = Vec::new();
        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::StringLiteral(lit) => {
                    collected.push((lit.value.as_str().to_string(), lit.span.start as usize));
                }
                AstKind::TemplateLiteral(tpl) => {
                    let s: String = tpl
                        .quasis
                        .iter()
                        .map(|q| q.value.raw.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    collected.push((s, tpl.span.start as usize));
                }
                _ => {}
            }
        }

        let mut diagnostics = Vec::new();
        for (i, (sel_text, _sel_offset)) in collected.iter().enumerate() {
            let Some(sel_table) = super::extract_select_from_table(sel_text) else {
                continue;
            };
            for (ins_text, ins_offset) in &collected[i + 1..] {
                let Some(ins_table) = super::extract_insert_into_table(ins_text) else {
                    continue;
                };
                if ins_table != sel_table {
                    continue;
                }
                if super::has_on_conflict(ins_text) {
                    break;
                }
                let (line, column) = byte_offset_to_line_col(ctx.source, *ins_offset);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "SELECT then INSERT on `{sel_table}` is a TOCTOU race — use `INSERT ... ON CONFLICT`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                break;
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_select_then_insert_same_table() {
        let src = "const a = `SELECT id FROM user WHERE email = $1`; const b = `INSERT INTO user (email) VALUES ($1)`;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_on_conflict() {
        let src = "const a = `SELECT id FROM user WHERE email = $1`; const b = `INSERT INTO user (email) VALUES ($1) ON CONFLICT (email) DO NOTHING`;";
        assert!(run_on(src).is_empty());
    }
}
