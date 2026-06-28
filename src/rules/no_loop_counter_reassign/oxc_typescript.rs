use std::sync::Arc;

use oxc_ast::AstKind;
use oxc_ast::ast::{AssignmentOperator, Expression, UpdateOperator};
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
            let Some(ForLoopSpans { body, update, stride }) = enclosing_for_spans(nodes, decl_id)
            else {
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
                // A counter write that advances in the SAME direction as the
                // update clause is an intentional fast-forward (the standard
                // "skip the next N elements" stride), not a confusing counter
                // mutation — leave it. Resets, arbitrary assignments, writes in
                // the opposite direction, and compound assignments whose
                // magnitude is not a positive integer literal stay flagged.
                if let Some(loop_stride) = stride
                    && write_stride(nodes.parent_node(reference.node_id()).kind())
                        == Some(loop_stride)
                {
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

/// The direction a `for` loop advances its counter, or the direction a body
/// write moves it. Used to tell an intentional same-direction stride apart
/// from a confusing counter mutation.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stride {
    Forward,
    Backward,
}

impl Stride {
    fn from_update(operator: UpdateOperator) -> Self {
        match operator {
            UpdateOperator::Increment => Stride::Forward,
            UpdateOperator::Decrement => Stride::Backward,
        }
    }

    /// The direction of a compound assignment's operator. `+=` advances
    /// forward, `-=` backward; any other compound operator (`*=`, `<<=`, …)
    /// has no monotonic direction.
    fn from_compound(operator: AssignmentOperator) -> Option<Self> {
        match operator {
            AssignmentOperator::Addition => Some(Stride::Forward),
            AssignmentOperator::Subtraction => Some(Stride::Backward),
            _ => None,
        }
    }
}

/// The body and update-clause spans of a C-style `for` statement, plus the
/// loop's iteration direction. The update span is `None` when the loop has an
/// empty update clause; `stride` is `None` when the update clause is empty or
/// is not a simple `++`/`--`/`+=`/`-=` (e.g. a comma sequence) — then no body
/// write is treated as a same-direction stride.
struct ForLoopSpans {
    body: Span,
    update: Option<Span>,
    stride: Option<Stride>,
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
                    stride: stmt.update.as_ref().and_then(update_stride),
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

/// The iteration direction encoded by a `for` loop's update clause.
fn update_stride(update: &Expression) -> Option<Stride> {
    match update {
        Expression::UpdateExpression(update) => Some(Stride::from_update(update.operator)),
        Expression::AssignmentExpression(assign) => Stride::from_compound(assign.operator),
        _ => None,
    }
}

/// The provable direction of a body write to the loop counter. `None` for a
/// reset or arbitrary assignment (`i = …`), and for a compound assignment
/// (`i += …`) whose right-hand side is not a positive integer literal — a
/// variable or expression RHS could be zero or negative at runtime, so forward
/// progress cannot be proven and the write stays flagged.
fn write_stride(kind: AstKind) -> Option<Stride> {
    match kind {
        AstKind::UpdateExpression(update) => Some(Stride::from_update(update.operator)),
        AstKind::AssignmentExpression(assign) => {
            let stride = Stride::from_compound(assign.operator)?;
            match &assign.right {
                Expression::NumericLiteral(literal)
                    if literal.value > 0.0 && literal.value.fract() == 0.0 =>
                {
                    Some(stride)
                }
                _ => None,
            }
        }
        _ => None,
    }
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

    // The issue's repro (unjs/citty): a body `i++` that skips a flag's value
    // argument advances in the same direction as the `i++` update clause, so it
    // is an intentional "skip next element" stride, not a counter corruption.
    #[test]
    fn forward_skip_in_incrementing_loop_is_allowed() {
        let source = r#"
function findSubCommandIndex(rawArgs: string[], argsDef: object): number {
    for (let i = 0; i < rawArgs.length; i++) {
        const arg = rawArgs[i]!;
        if (arg === "--") return -1;
        if (arg.startsWith("-")) {
            if (!arg.includes("=") && isValueFlag(arg, argsDef)) {
                i++;
            }
            continue;
        }
        return i;
    }
    return -1;
}
"#;
        assert!(run(source).is_empty());
    }

    // A descending loop symmetrically allows a same-direction `i--` stride.
    #[test]
    fn backward_skip_in_decrementing_loop_is_allowed() {
        let source = r#"
for (let i = n; i > 0; i--) {
    if (cond) {
        i--;
    }
}
"#;
        assert!(run(source).is_empty());
    }

    // `i += 2` advances forward by a positive integer literal in an
    // incrementing loop — same direction, provable forward progress.
    #[test]
    fn forward_compound_literal_stride_is_allowed() {
        let source = "for (let i = 0; i < n; i++) { i += 2; }\n";
        assert!(run(source).is_empty());
    }

    // A write in the OPPOSITE direction (decrement in an incrementing loop) is
    // exactly the confusing mutation the rule exists to catch.
    #[test]
    fn opposite_direction_write_is_flagged() {
        let source = "for (let i = 0; i < n; i++) { i--; }\n";
        assert_eq!(run(source).len(), 1);
    }

    // A reset to a constant breaks the counted-iteration contract.
    #[test]
    fn reset_is_flagged() {
        let source = "for (let i = 0; i < n; i++) { if (cond) i = 0; }\n";
        assert_eq!(run(source).len(), 1);
    }

    // An arbitrary assignment from another value is still flagged.
    #[test]
    fn arbitrary_assignment_is_flagged() {
        let source = "for (let i = 0; i < n; i++) { i = someValue; }\n";
        assert_eq!(run(source).len(), 1);
    }

    // A compound assignment whose magnitude is a variable (not a positive
    // integer literal) cannot be proven to move forward — it could be zero or
    // negative at runtime — so it stays flagged.
    #[test]
    fn non_literal_compound_is_flagged() {
        let source = "for (let i = 0; i < n; i++) { i += step; }\n";
        assert_eq!(run(source).len(), 1);
    }

    // `i -= 2` in a decrementing loop is a same-direction literal stride.
    #[test]
    fn backward_compound_literal_stride_is_allowed() {
        let source = "for (let i = n; i > 0; i -= 2) { i -= 2; }\n";
        assert!(run(source).is_empty());
    }

    // A compound assignment in the OPPOSITE direction (`i -= 2` in an
    // incrementing loop) is flagged despite its literal magnitude.
    #[test]
    fn opposite_direction_compound_is_flagged() {
        let source = "for (let i = 0; i < n; i++) { i -= 2; }\n";
        assert_eq!(run(source).len(), 1);
    }

    // `i = i + 1` is a plain assignment, not an increment expression: it is
    // flagged even though it is semantically a forward step, because only
    // `i++`/`++i`/`i += <literal>` are recognised as strides.
    #[test]
    fn self_referential_assignment_is_flagged() {
        let source = "for (let i = 0; i < n; i++) { i = i + 1; }\n";
        assert_eq!(run(source).len(), 1);
    }

    // A comma-sequence update clause has no single direction, so the stride
    // exemption is disabled and a body write is flagged.
    #[test]
    fn sequence_update_clause_flags_body_write() {
        let source = "for (let i = 0; i < n; i++, j++) { i++; }\n";
        assert_eq!(run(source).len(), 1);
    }
}
