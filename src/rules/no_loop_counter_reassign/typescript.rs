//! no-loop-counter-reassign — TS / JS / TSX backend.
//!
//! Walks every binding declared in a `for (let i = …)` /
//! `for (let x of …)` / `for (let k in …)` header, then checks the
//! associated symbol's resolved references. Any write reference whose
//! source position falls inside the loop body (and not the header
//! itself) is flagged.
//!
//! Replaces a brittle line-based scan that:
//! - parsed `for (` from raw source text, mishandling multi-line
//!   loop headers,
//! - matched the counter name with `find()` (substring), so a
//!   reassignment to `iteration` would over-fire when the counter
//!   was `i`,
//! - tracked block depth by counting `{` / `}` characters, breaking
//!   on object literals and template strings inside the body.

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
                let Some(body_span) = enclosing_for_body_span(nodes, decl_id) else {
                    continue;
                };
                // Only treat the symbol as a loop counter when its
                // declaration sits in the for-header (not in the body).
                let decl_span = nodes.kind(decl_id).span();
                if span_contains(body_span, decl_span) {
                    continue;
                }

                let name = scoping.symbol_name(symbol_id);
                for reference in scoping.get_resolved_references(symbol_id) {
                    if !reference.flags().contains(ReferenceFlags::Write) {
                        continue;
                    }
                    let ref_span = nodes.kind(reference.node_id()).span();
                    if !span_contains(body_span, ref_span) {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, ref_span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
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

/// Find the body span of the nearest enclosing `for` statement whose
/// init declares the symbol at `decl_id`. Stops at the first
/// function / arrow / program ancestor.
fn enclosing_for_body_span(
    nodes: &oxc_semantic::AstNodes,
    decl_id: oxc_semantic::NodeId,
) -> Option<Span> {
    for kind in nodes.ancestor_kinds(decl_id) {
        match kind {
            AstKind::ForStatement(stmt) => return Some(stmt.body.span()),
            AstKind::ForOfStatement(stmt) => return Some(stmt.body.span()),
            AstKind::ForInStatement(stmt) => return Some(stmt.body.span()),
            AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::Program(_) => return None,
            _ => {}
        }
    }
    None
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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
    fn flags_for_of_counter_reassign() {
        let src = "for (let k of items) {\n  k = 'x';\n}";
        assert_eq!(run_on(src).len(), 1);
    }
}
