use std::sync::Arc;

use oxc_ast::AstKind;
use oxc_semantic::ReferenceFlags;
use oxc_span::{GetSpan, Span};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        for symbol_id in scoping.symbol_ids() {
            let decl_id = scoping.symbol_declaration(symbol_id);
            let Some(ForLoopSpans { body, update }) = enclosing_for_spans(nodes, decl_id) else {
                continue;
            };
            let decl_span = nodes.kind(decl_id).span();
            if span_contains(body, decl_span) {
                continue;
            }
            // The counter is the variable the loop advances in its update
            // clause. A for-init binding the update clause does not
            // reference is an accumulator, and recomputing it in the body
            // is intended — skip it.
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
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Loop counter `{name}` is reassigned inside the loop body."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

/// The body and update-clause spans of a C-style `for` statement. The
/// update span is `None` when the loop has an empty update clause.
struct ForLoopSpans {
    body: Span,
    update: Option<Span>,
}

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
