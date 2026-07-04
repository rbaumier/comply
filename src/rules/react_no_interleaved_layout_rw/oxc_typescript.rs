//! OXC backend for react-no-interleaved-layout-rw.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::Span;
use std::sync::Arc;

const LAYOUT_READ_PROPS: &[&str] = &[
    "offsetWidth",
    "offsetHeight",
    "offsetTop",
    "offsetLeft",
    "clientWidth",
    "clientHeight",
    "scrollTop",
    "scrollLeft",
    "scrollWidth",
    "scrollHeight",
    "getBoundingClientRect",
    "getClientRects",
];

#[derive(Clone, Copy, PartialEq)]
enum Op {
    Read,
    Write,
}

pub struct Check;

/// True when this layout-property member expression is the left-hand target of
/// a plain `=` assignment (e.g. `El.prototype.getBoundingClientRect = fn`).
/// Assigning *to* the property slot overrides/mocks it — that is a write to the
/// slot, not a layout read, so it must not count as a read. Compound
/// assignments (`+=`, `-=`, …) genuinely read the slot before writing, so this
/// returns false for them and the read is still recorded.
fn is_plain_assignment_lhs(parent: AstKind, member_span: Span) -> bool {
    let AstKind::AssignmentExpression(assign) = parent else {
        return false;
    };
    if assign.operator != oxc_ast::ast::AssignmentOperator::Assign {
        return false;
    }
    // Only the assignment *target* (left) is a write; an identical layout read
    // on the right-hand side (`a.width = b.offsetWidth`) keeps its own span and
    // is not suppressed.
    matches!(
        &assign.left,
        oxc_ast::ast::AssignmentTarget::StaticMemberExpression(left) if left.span == member_span
    )
}

fn is_interleaved(ops: &[Op]) -> bool {
    if ops.len() < 3 {
        return false;
    }
    let mut runs = 1;
    for w in ops.windows(2) {
        if w[0] != w[1] {
            runs += 1;
        }
    }
    runs >= 3
}

impl OxcCheck for Check {
    // Per function we need the layout-read / style-write op sequence scoped to
    // its own body (excluding nested function scopes). A per-node `run` would
    // walk the whole AST twice for every Function / ArrowFunctionExpression
    // (O(functions × nodes)); instead collect every function scope and op once
    // per file via `run_on_semantic`, then resolve each function's ops by span
    // containment.
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Single pre-order pass: every function/arrow node span (for nested-scope
        // exclusion), each one's body span (only when it has a body — the units
        // we actually analyse), and every layout op in source order.
        let mut all_fn_spans: Vec<Span> = Vec::new();
        let mut analyze: Vec<(Span, Span)> = Vec::new(); // (node span, body span)
        let mut ops: Vec<(Span, Op)> = Vec::new();
        for n in semantic.nodes().iter() {
            match n.kind() {
                AstKind::Function(func) => {
                    all_fn_spans.push(func.span);
                    if let Some(body) = &func.body {
                        analyze.push((func.span, body.span));
                    }
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    all_fn_spans.push(arrow.span);
                    analyze.push((arrow.span, arrow.body.span));
                }
                AstKind::StaticMemberExpression(member) => {
                    if LAYOUT_READ_PROPS.contains(&member.property.name.as_str())
                        && !is_plain_assignment_lhs(
                            semantic.nodes().parent_node(n.id()).kind(),
                            member.span,
                        )
                    {
                        ops.push((member.span, Op::Read));
                    }
                }
                AstKind::AssignmentExpression(assign) => {
                    if let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(left) = &assign.left
                        && let Expression::StaticMemberExpression(obj_member) = &left.object
                        && obj_member.property.name.as_str() == "style"
                    {
                        ops.push((assign.span, Op::Write));
                    }
                }
                _ => {}
            }
        }

        let mut diagnostics = Vec::new();
        for (node_span, body_span) in &analyze {
            // Functions strictly nested inside this body are skipped (their ops
            // belong to that inner scope). The current function excludes itself:
            // its node span starts before its body span.
            let nested: Vec<Span> = all_fn_spans
                .iter()
                .copied()
                .filter(|s| s.start > body_span.start && s.end <= body_span.end)
                .collect();
            let fn_ops: Vec<Op> = ops
                .iter()
                .filter(|(s, _)| {
                    s.start >= body_span.start
                        && s.end <= body_span.end
                        && !nested.iter().any(|fs| s.start >= fs.start && s.end <= fs.end)
                })
                .map(|(_, op)| *op)
                .collect();
            if !is_interleaved(&fn_ops) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, node_span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Layout reads (e.g. `offsetWidth`, `getBoundingClientRect`) interleaved \
                         with `.style.*` writes force sync layout. Batch reads first, writes second."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_layout_prop_overrides_around_a_write() {
        // The layout-property name only ever appears on the LHS of `=`
        // (mocking / restoring a method), so there are zero real reads.
        let src = r#"
function overrideMock() {
  HTMLElement.prototype.getBoundingClientRect = mockGetBoundingClientRect;
  el.style.width = "300px";
  HTMLElement.prototype.getBoundingClientRect = originalGetBoundingClientRect;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_genuine_interleaved_read_write() {
        let src = r#"
function thrash() {
  const w = el.offsetWidth;
  el.style.width = w + "px";
  const h = el.offsetHeight;
  el.style.height = h + "px";
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn compound_assignment_lhs_still_reads() {
        // `+=` reads the slot before writing, so its LHS still counts as a
        // read and interleaving with a `.style.*` write is flagged.
        let src = r#"
function bump() {
  el.scrollTop += 5;
  el.style.top = "0px";
  el.scrollTop += 5;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn rhs_layout_read_in_assignment_is_not_suppressed() {
        // The layout read is a bare RHS member (`= b.offsetWidth`) whose parent
        // *is* the assignment; only the LHS target span is suppressed, so the
        // `left.span == member.span` check must keep this read — the sequence
        // still interleaves.
        let src = r#"
function copy() {
  a.style.width = b.offsetWidth;
  b.style.height = a.offsetHeight;
  a.style.left = b.offsetLeft;
}
"#;
        assert_eq!(run(src).len(), 1);
    }
}
