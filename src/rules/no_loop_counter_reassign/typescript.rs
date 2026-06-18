//! no-loop-counter-reassign — TS / JS / TSX backend.
//!
//! Walks every binding declared in a C-style `for (let i = …; …; …)`
//! header, then checks the associated symbol's resolved references. Any
//! write reference whose source position falls inside the loop body
//! (and not the header itself) is flagged. `for-of` / `for-in` head
//! bindings are element/key bindings, not counters — the spec rebinds
//! them each iteration, so reassigning them inside the body has no
//! effect on iteration and is never flagged.

use oxc_ast::AstKind;
use oxc_semantic::ReferenceFlags;
use oxc_span::{GetSpan, Span};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{source_type_for_path, with_semantic};
use crate::rules::backend::CheckCtx;

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_type = source_type_for_path(ctx.path);
        with_semantic(ctx.source, source_type, |semantic| {
            let scoping = semantic.scoping();
            let nodes = semantic.nodes();
            let mut diagnostics = Vec::new();

            for symbol_id in scoping.symbol_ids() {
                let decl_id = scoping.symbol_declaration(symbol_id);
                let Some(ForLoopSpans { body, update }) = enclosing_for_spans(nodes, decl_id)
                else {
                    continue;
                };
                // Only treat the symbol as a loop counter when its
                // declaration sits in the for-header (not in the body).
                let decl_span = nodes.kind(decl_id).span();
                if span_contains(body, decl_span) {
                    continue;
                }
                // The counter is the variable the loop advances in its
                // update clause. A for-init binding the update clause does
                // not reference is an accumulator, and recomputing it in
                // the body is intended — skip it.
                if !is_referenced_in_update(scoping, symbol_id, update, nodes) {
                    continue;
                }

                let name = scoping.symbol_name(symbol_id);
                for reference in scoping.get_resolved_references(symbol_id) {
                    if !reference.flags().contains(ReferenceFlags::Write) {
                        continue;
                    }
                    let ref_span = nodes.kind(reference.node_id()).span();
                    if !span_contains(body, ref_span) {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, ref_span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "no-loop-counter-reassign".into(),
                        message: format!(
                            "Loop counter `{name}` is reassigned inside the loop body."
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }

            diagnostics
        })
    }
}

/// The body and update-clause spans of a C-style `for` statement. The
/// update span is `None` when the loop has an empty update clause.
struct ForLoopSpans {
    body: Span,
    update: Option<Span>,
}

/// Find the spans of the nearest enclosing C-style `for` statement whose
/// init declares the symbol at `decl_id`. Stops at the first function /
/// arrow / program ancestor. `for-of` / `for-in` are not counted loops,
/// so their head bindings never match.
fn enclosing_for_spans(
    nodes: &oxc_semantic::AstNodes,
    decl_id: oxc_semantic::NodeId,
) -> Option<ForLoopSpans> {
    for kind in nodes.ancestor_kinds(decl_id) {
        match kind {
            AstKind::ForStatement(stmt) => {
                return Some(ForLoopSpans {
                    body: stmt.body.span(),
                    update: stmt.update.as_ref().map(GetSpan::span),
                });
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return None;
            }
            _ => {}
        }
    }
    None
}

/// A for-init binding is the loop counter only when the update clause
/// references it (e.g. `i` in `i++`). Returns `false` for an empty update
/// clause, since then nothing advances and the body reassignment is the
/// progression itself.
fn is_referenced_in_update(
    scoping: &oxc_semantic::Scoping,
    symbol_id: oxc_semantic::SymbolId,
    update: Option<Span>,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    let Some(update) = update else {
        return false;
    };
    scoping
        .get_resolved_references(symbol_id)
        .any(|reference| span_contains(update, nodes.kind(reference.node_id()).span()))
}

fn span_contains(outer: Span, inner: Span) -> bool {
    inner.start >= outer.start && inner.end <= outer.end
}

fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_counter_reassign() {
        let src = "for (let i = 0; i < n; i++) {\n  i = 5;\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_counter_reassign_var() {
        let src = "for (var j = 0; j < 10; j++) {\n  j = 0;\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_normal_loop() {
        let src = "for (let i = 0; i < n; i++) {\n  console.log(i);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_different_var() {
        let src = "for (let i = 0; i < n; i++) {\n  x = 5;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_object_literal_with_curly_braces() {
        // The previous text scan tracked `{` / `}` to find the body
        // end and broke when an object literal closed the depth too
        // early. The semantic version uses real spans.
        let src = "for (let i = 0; i < n; i++) {\n  const obj = { i, k: 1 };\n  use(obj);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_substring_name_collision() {
        // A reassignment to `iteration` should not be picked up when
        // the counter is `i` — the text-scan used `find(var_name)`
        // and would over-fire.
        let src = "for (let i = 0; i < n; i++) {\n  let iteration = 0; iteration = 1;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_for_of_binding_reassign() {
        // A `for-of` head binding is re-initialised each iteration, so
        // reassigning it inside the body has no effect on iteration —
        // it is an idiomatic local transformation, not a counter bug.
        // Mirrors elysia `for (let [path, route] of Object.entries(x))
        // { path = encodeURI(path) }`.
        let src =
            "for (let [path, route] of Object.entries(x)) {\n  path = encodeURI(path);\n  use(route);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_for_in_binding_value_reassign() {
        let src = "for (const k in obj) {\n  let value = obj[k];\n  value = f(value);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_outer_c_counter_reassigned_inside_for_of() {
        // The reassigned symbol IS the outer C-style counter `i`, so
        // the for-of nesting does not shield it.
        let src =
            "for (let i = 0; i < n; i++) {\n  for (const x of items) {\n    i = 5;\n  }\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_for_of_binding_reassign_inside_c_loop() {
        // `x` is the for-of element binding, not the C-style counter, so
        // reassigning it stays clean even nested inside a `for`.
        let src =
            "for (let i = 0; i < n; i++) {\n  for (const x of items) {\n    x = f(x);\n  }\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_init_accumulator_not_in_update() {
        // `tick` is declared in the for-init but the update clause only
        // advances `idx`, so `tick` is a loop-scoped accumulator the body
        // is supposed to recompute. Mirrors echarts `Interval.ts`.
        let src = "function f(start, interval, count) {\n  const out = [];\n  for (let tick = start, idx = 0; ; idx++) {\n    if (idx > count) break;\n    out.push(tick);\n    tick = tick + interval;\n  }\n  return out;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_init_var_when_update_clause_empty() {
        // An empty update clause advances nothing, so the body
        // reassignment is the sole progression mechanism. Mirrors mantine
        // `TreeNode.tsx` `for (let cur = node; cur; ) { cur = cur.parentElement; }`.
        let src = "for (let cur = node; cur; ) {\n  cur = cur.parentElement;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_counter_with_second_init_var() {
        // The counter `i` IS referenced in the `i++` update, so a body
        // reassignment of `i` is still a genuine off-by-one risk and must
        // flag, even when a sibling accumulator shares the init clause.
        let src = "for (let i = 0, acc = 0; i < n; i++) {\n  acc = acc + i;\n  i = i + 2;\n}";
        assert_eq!(run_on(src).len(), 1);
    }
}
