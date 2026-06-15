//! sql-no-select-then-insert-race — oxc backend for TS / JS / TSX.
//!
//! Collects the string/template literals in a file together with their
//! enclosing function, then flags a SELECT followed by an INSERT on the same
//! table — without ON CONFLICT — only when both occur in the same function
//! body. A TOCTOU race is a check-then-act sequence within one execution path;
//! a SELECT in one function and an INSERT in a separate function never run in
//! sequence and are not a race.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::Span;
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
        // Each literal is keyed by the span of its nearest enclosing function
        // (`None` = module top level). Cross-matching is confined to a single
        // scope so two independent functions are never paired.
        struct Literal {
            text: String,
            offset: usize,
            scope: Option<Span>,
        }
        let mut collected: Vec<Literal> = Vec::new();
        for node in semantic.nodes().iter() {
            let (text, offset) = match node.kind() {
                AstKind::StringLiteral(lit) => {
                    (lit.value.as_str().to_string(), lit.span.start as usize)
                }
                AstKind::TemplateLiteral(tpl) => {
                    let s: String = tpl
                        .quasis
                        .iter()
                        .map(|q| q.value.raw.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    (s, tpl.span.start as usize)
                }
                _ => continue,
            };
            let scope = semantic.nodes().ancestors(node.id()).find_map(|ancestor| {
                match ancestor.kind() {
                    AstKind::Function(f) => Some(f.span),
                    AstKind::ArrowFunctionExpression(a) => Some(a.span),
                    _ => None,
                }
            });
            collected.push(Literal { text, offset, scope });
        }

        let mut diagnostics = Vec::new();
        for (i, sel) in collected.iter().enumerate() {
            let Some(sel_table) = super::extract_select_from_table(&sel.text) else {
                continue;
            };
            for ins in &collected[i + 1..] {
                if ins.scope != sel.scope {
                    continue;
                }
                let Some(ins_table) = super::extract_insert_into_table(&ins.text) else {
                    continue;
                };
                if ins_table != sel_table {
                    continue;
                }
                if super::has_on_conflict(&ins.text) {
                    break;
                }
                let (line, column) = byte_offset_to_line_col(ctx.source, ins.offset);
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

    #[test]
    fn ignores_select_and_insert_in_separate_functions() {
        // Issue #2210: a SELECT in getPosts() and an INSERT in createPost() are
        // independent functions, never executed in sequence — not a TOCTOU race.
        let src = r#"
            export async function getPosts(db) {
              return db.prepare('SELECT * FROM posts ORDER BY created_at DESC').all();
            }
            export async function createPost(db, title) {
              return db.prepare('INSERT INTO posts (title) VALUES (?)').bind(title).run();
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_select_then_insert_within_same_function() {
        // Negative space: a genuine check-then-act in one function still flags.
        let src = r#"
            async function upsert(db, email) {
              const existing = await db.query('SELECT id FROM user WHERE email = $1', [email]);
              if (existing) return existing;
              return db.query('INSERT INTO user (email) VALUES ($1)', [email]);
            }
        "#;
        assert_eq!(run_on(src).len(), 1);
    }
}
