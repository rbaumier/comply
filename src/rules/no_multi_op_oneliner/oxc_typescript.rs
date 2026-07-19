//! no-multi-op-oneliner oxc backend.

use rustc_hash::FxHashSet;
use oxc_ast::ast::Expression;
use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Number of function calls in `expr` if it is a single arithmetic formula,
/// or `None` if it is not.
///
/// A formula is a tree built only from numeric literals, identifiers,
/// member/index access (`opts.damping`, `old[j]`, `S[i * n + j]`), unary
/// `+`/`-`, `+ - * / % **` binary operators (grouping parens transparent),
/// and function calls whose arguments are themselves formulas. Member and
/// index access are leaves: indexing/property access is addressing, not a
/// nameable readability step, so the arithmetic inside an index does not
/// break the formula.
///
/// Horner-form polynomial evaluation (`a + t * (b + t * (c + ...))`) and the
/// affinity-propagation matrix update (`(1 - d) * (S[i] - max) + d * old[j]`)
/// are the canonical cases — one mathematical formula, not a chain of
/// nameable steps. Callers collapse such a span to a single operation, but
/// only when the call count is at most one: a single wrapped builtin
/// (`Math.min(0, sum - x)`) is cohesive, whereas several independent calls
/// (`scale(b) + offset(x)`) are a dense chain that must stay flagged.
fn arithmetic_call_count(expr: &Expression) -> Option<usize> {
    match expr.without_parentheses() {
        Expression::NumericLiteral(_) | Expression::Identifier(_) => Some(0),
        Expression::StaticMemberExpression(_)
        | Expression::ComputedMemberExpression(_) => Some(0),
        Expression::UnaryExpression(unary) if unary.operator.is_arithmetic() => {
            arithmetic_call_count(&unary.argument)
        }
        Expression::BinaryExpression(bin) if bin.operator.is_arithmetic() => {
            Some(arithmetic_call_count(&bin.left)? + arithmetic_call_count(&bin.right)?)
        }
        Expression::CallExpression(call) => {
            let mut calls = 1;
            for arg in &call.arguments {
                // A spread argument (`f(...xs)`) is not formula-arithmetic.
                let inner = arg.as_expression()?;
                calls += arithmetic_call_count(inner)?;
            }
            Some(calls)
        }
        _ => None,
    }
}

/// True when `expr` is a single arithmetic formula whose call count is within
/// the single-call budget, i.e. collapsible to one operation.
fn is_collapsible_formula(expr: &Expression) -> bool {
    arithmetic_call_count(expr).is_some_and(|calls| calls <= 1)
}

/// True when the arithmetic span at `node` sits inside a call argument, rather
/// than in the value position of its statement.
///
/// A formula in value position (`x = <formula>`, `return <formula>`) collapses
/// to one operation. A formula sitting inside a call argument — `a - b` in
/// `Math.hypot(a - b, c - d)`, or the inner term of a non-collapsing multi-call
/// chain — is a separate step whose collapse would wrongly suppress a genuinely
/// dense line; the enclosing call already accounts for it. A call that is part
/// of a collapsing formula keeps its own maximal span, so its arguments are
/// absorbed there rather than subtracted twice.
fn is_buried_in_call(node_id: oxc_semantic::NodeId, semantic: &oxc_semantic::Semantic) -> bool {
    semantic.nodes().ancestors(node_id).any(|ancestor| {
        matches!(
            ancestor.kind(),
            AstKind::CallExpression(_) | AstKind::NewExpression(_)
        )
    })
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Test assertions (Playwright, Vitest) are inherently chained.
        if ctx.file.path_segments.in_test_dir {
            return Vec::new();
        }
        let min_ops = ctx
            .config
            .threshold("no-multi-op-oneliner", "min_ops", ctx.lang);
        let min_line_length = ctx
            .config
            .threshold("no-multi-op-oneliner", "min_line_length", ctx.lang);

        let line_offsets = super::dense_lines::compute_line_offsets(ctx.source);

        // Collect byte ranges to strip before counting operators: comments and
        // regex literals. A regex pattern's `-`, `/`, and quantifier syntax are
        // not chained operations — stripping them mirrors the string-literal skip
        // already done in `count_operators`.
        let mut strip_ranges: Vec<(usize, usize)> = semantic
            .comments()
            .iter()
            .map(|c| (c.span.start as usize, c.span.end as usize))
            .collect();
        for node in semantic.nodes() {
            if let AstKind::RegExpLiteral(re) = node.kind() {
                strip_ranges.push((re.span.start as usize, re.span.end as usize));
            }
        }

        // Spans of arithmetic formulas: a binary-arithmetic tree over numeric
        // literals, identifiers, member/index access, and at most one wrapped
        // call. Each one is a single mathematical formula, so its operators
        // count as one operation rather than as many chained steps. A
        // compound-arithmetic assignment (`x += <formula>`) is itself a formula
        // root, so its whole span — including the left-hand index arithmetic —
        // collapses too. Collect every such span, then keep only the maximal
        // ones (a nested sub-formula's operators must not be subtracted twice).
        let mut all_spans: Vec<(usize, usize)> = Vec::new();
        for node in semantic.nodes() {
            match node.kind() {
                AstKind::BinaryExpression(bin) if bin.operator.is_arithmetic() => {
                    // The whole binary — both operands together — must be a
                    // formula within the single-call budget, not just each
                    // operand independently (`foo(a) + bar(b)` is two calls).
                    let whole = arithmetic_call_count(&bin.left)
                        .zip(arithmetic_call_count(&bin.right))
                        .map(|(l, r)| l + r);
                    if whole.is_some_and(|calls| calls <= 1)
                        && !is_buried_in_call(node.id(), semantic)
                    {
                        all_spans.push((bin.span.start as usize, bin.span.end as usize));
                    }
                }
                AstKind::AssignmentExpression(assign)
                    if assign.operator.is_arithmetic()
                        && is_collapsible_formula(&assign.right)
                        && !is_buried_in_call(node.id(), semantic) =>
                {
                    all_spans.push((assign.span.start as usize, assign.span.end as usize));
                }
                _ => {}
            }
        }
        let formula_spans: Vec<(usize, usize)> = all_spans
            .iter()
            .filter(|&&(s, e)| {
                !all_spans
                    .iter()
                    .any(|&(os, oe)| os <= s && oe >= e && (os, oe) != (s, e))
            })
            .copied()
            .collect();

        let mut reported_lines = FxHashSet::default();
        let mut diagnostics = Vec::new();

        for node in semantic.nodes() {
            let is_target = matches!(
                node.kind(),
                AstKind::ExpressionStatement(_) | AstKind::VariableDeclarator(_)
            );
            if !is_target {
                continue;
            }
            let span = match node.kind() {
                AstKind::ExpressionStatement(s) => s.span,
                AstKind::VariableDeclarator(d) => d.span,
                _ => continue,
            };
            let (start_line, _) = byte_offset_to_line_col(ctx.source, span.start as usize);
            let (end_line, _) = byte_offset_to_line_col(ctx.source, span.end as usize);
            if start_line != end_line {
                continue;
            }
            if reported_lines.contains(&start_line) {
                continue;
            }
            // line_offsets is 0-indexed, start_line is 1-indexed.
            let Some(&(line_start_byte, line_text)) = line_offsets.get(start_line - 1) else {
                continue;
            };
            let stripped =
                super::dense_lines::strip_comments(line_text, line_start_byte, &strip_ranges);
            if stripped.len() < min_line_length {
                continue;
            }
            let mut ops = super::dense_lines::count_operators(&stripped);
            // Collapse each pure-arithmetic formula on this line to a single
            // operation: subtract its operator bytes, add one. A formula is one
            // mathematical computation, not a chain of nameable steps.
            let line_end_byte = line_start_byte + line_text.len();
            for &(fs, fe) in &formula_spans {
                let lo = fs.max(line_start_byte);
                let hi = fe.min(line_end_byte);
                if lo >= hi {
                    continue;
                }
                // Strip the same comment/regex ranges `count_operators` honored,
                // so a comment between formula tokens (`a + /* */ b`) does not
                // over-count the formula's operators and eat unrelated ops.
                let slice = &line_text[lo - line_start_byte..hi - line_start_byte];
                let slice = super::dense_lines::strip_comments(slice, lo, &strip_ranges);
                let formula_ops = super::dense_lines::count_arithmetic_bytes(&slice);
                ops = ops.saturating_sub(formula_ops) + 1;
            }
            if ops < min_ops {
                continue;
            }
            reported_lines.insert(start_line);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: start_line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Line has {ops} chained operations — extract intermediate \
                     named bindings so each step's purpose is visible."
                ),
                severity: Severity::Error,
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
    
    #[test]
    fn ignores_regex_literal_character_classes() {
        // Regression for issue #524: a regex literal's `-`, `/` and quantifier
        // syntax are not chained operations.
        let src = "const UUIDV7_REGEX = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[0-9a-f]{4}-[0-9a-f]{12}$/;";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty(), "got {:?}", crate::rules::test_helpers::run_rule(&Check, src, "t.ts"));
    }

    #[test]
    fn still_flags_dense_operator_chain() {
        let src = "const total = items.map(x => x.price).filter(p => p > 0).reduce((a, b) => a + b, 0) + extra;";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").len(), 1);
    }

    #[test]
    fn ignores_horner_polynomial_evaluation() {
        // Regression for issue #5680: Horner-form polynomial evaluation is a
        // single arithmetic expression (numeric literals + one variable +
        // `+ - * /` + grouping parens), not a chain of nameable steps.
        let src = "const dt = 63.86 + t * (0.3345 + t * (-0.060374 + t * (0.0017275 + t * (0.000651814 + t * 0.00002373599))));";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty(),
            "got {:?}",
            crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
        );
    }

    #[test]
    fn ignores_horner_polynomial_with_leading_assignment() {
        // The issue's exact line: an assignment followed by a `return` of a
        // Horner polynomial, both on one line. The assignment statement is the
        // flagged node, but the line's operators are dominated by the formula.
        let src = "if (y < 2005) { t = y - 2000; return 63.86 + t * (0.3345 + t * (-0.060374 + t * (0.0017275 + t * (0.000651814 + t * 0.00002373599)))); }";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty(),
            "got {:?}",
            crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
        );
    }

    #[test]
    fn still_flags_arithmetic_over_call_operands() {
        // Guards `is_pure_arithmetic` against over-broadening: arithmetic whose
        // operands are call expressions is NOT a single formula — each call is a
        // distinct nameable step — so nothing collapses and the line still flags.
        let src = "const out = scale(base) + offset(x) * weight(y) - clamp(lo) / norm(hi) + bias(z) * gain(w);";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").len(), 1);
    }

    #[test]
    fn comment_inside_formula_does_not_suppress_dense_line() {
        // A comment's operator-like bytes (`/*//////*/`) sit inside the
        // `aa + bb` formula span. They must not be counted as formula operators
        // and subtracted, or they would eat the trailing call chain's ops and
        // wrongly suppress a genuine dense one-liner.
        let src = "const out = aa + /*//////*/ bb; const q = ff(xx).gg(yy).hh(zz).ii(ww).jj(vv).kk(uu).ll(tt);";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").len(), 1);
    }

    #[test]
    fn ignores_matrix_update_with_index_and_member_access() {
        // Regression for issue #5729: the affinity-propagation responsibility
        // update is one published formula. Index arithmetic (`i * n + j`) and
        // property access (`opts.damping`, `old[j]`) are addressing, not
        // nameable operations, so the whole RHS collapses to one operation.
        let src = "R[i * n + j] = (1 - opts.damping) * (S[i * n + j] - max) + opts.damping * old[j];";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty(),
            "got {:?}",
            crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
        );
    }

    #[test]
    fn ignores_matrix_update_with_single_math_call() {
        // Regression for issue #5729: the availability update wraps one
        // `Math.min(...)` over arithmetic args. A single cohesive call inside a
        // formula is part of the formula, so the RHS still collapses.
        let src = "A[j * n + i] = (1 - opts.damping) * Math.min(0, sum - Rp[j]) + opts.damping * old[j];";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty(),
            "got {:?}",
            crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
        );
    }

    #[test]
    fn ignores_compound_assignment_formula() {
        // Regression for issue #5729: a compound-assignment accumulator is a
        // single formula root, no different from `x = x + ...`.
        let src = "accumulator[k] += (1 - decay) * (sample[k] - mean[k]) * weight + bias[k];";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty(),
            "got {:?}",
            crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
        );
    }

    #[test]
    fn still_flags_mermaid_reduce_plus_gap_chain() {
        // Issue #5729 defensible-flag case: a reduction closure plus a separate
        // gap term is a dense chain of independent steps (a `reduce` over an
        // arrow closure and a second `Math.max` call), not one formula.
        let src = "const total = ids.reduce((s, id) => s + getWidth(id), 0) + nodeGap * Math.max(0, ids.length - 1);";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").len(), 1);
    }

    #[test]
    fn still_flags_multi_call_arithmetic_chain() {
        // Two independent calls in one arithmetic span exceed the single-call
        // budget, so the span does not collapse and the dense line still flags.
        let src = "const score = computeBase(input) * factorWeight(x) + computeOffset(y) - penalty(z) / norm(w);";
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").len(), 1);
    }
}
