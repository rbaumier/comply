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
            let Some(body_span) = enclosing_for_body_span(nodes, decl_id) else {
                continue;
            };
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

fn enclosing_for_body_span(
    nodes: &oxc_semantic::AstNodes,
    decl_id: oxc_semantic::NodeId,
) -> Option<Span> {
    for kind in nodes.ancestor_kinds(decl_id) {
        match kind {
            AstKind::ForStatement(stmt) => return Some(stmt.body.span()),
            AstKind::ForOfStatement(stmt) => return Some(stmt.body.span()),
            AstKind::ForInStatement(stmt) => return Some(stmt.body.span()),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return None;
            }
            _ => {}
        }
    }
    None
}

fn span_contains(outer: Span, inner: Span) -> bool {
    inner.start >= outer.start && inner.end <= outer.end
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
