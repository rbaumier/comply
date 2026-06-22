//! no-multi-op-oneliner oxc backend.

use rustc_hash::FxHashSet;
use oxc_ast::ast::Expression;
use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// True when `expr` is a single arithmetic computation: a tree built only
/// from numeric literals, identifiers, unary `+`/`-`, and `+ - * / % **`
/// binary operators (with grouping parens). Horner-form polynomial
/// evaluation (`a + t * (b + t * (c + ...))`) is the canonical case — it is
/// one mathematical formula, not a chain of nameable steps, so its operators
/// collapse to a single operation for the density count.
fn is_pure_arithmetic(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::NumericLiteral(_) | Expression::Identifier(_) => true,
        Expression::UnaryExpression(unary) => {
            unary.operator.is_arithmetic() && is_pure_arithmetic(&unary.argument)
        }
        Expression::BinaryExpression(bin) => {
            bin.operator.is_arithmetic()
                && is_pure_arithmetic(&bin.left)
                && is_pure_arithmetic(&bin.right)
        }
        _ => false,
    }
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

        // Spans of pure-arithmetic expressions: a binary-arithmetic tree over
        // numeric literals and identifiers. Each one is a single mathematical
        // formula, so its operators count as one operation rather than as many
        // chained steps. Collect every such span, then keep only the maximal
        // ones (a nested sub-formula's operators must not be subtracted twice).
        let mut all_spans: Vec<(usize, usize)> = Vec::new();
        for node in semantic.nodes() {
            let AstKind::BinaryExpression(bin) = node.kind() else {
                continue;
            };
            if bin.operator.is_arithmetic()
                && is_pure_arithmetic(&bin.left)
                && is_pure_arithmetic(&bin.right)
            {
                all_spans.push((bin.span.start as usize, bin.span.end as usize));
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
}
