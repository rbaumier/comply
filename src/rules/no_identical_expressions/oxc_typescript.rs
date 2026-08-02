//! no-identical-expressions oxc backend.
//!
//! Identical source text proves identical values only for reproducible
//! operands, so both sides must satisfy
//! [`oxc_helpers::expression_is_reproducible`](crate::oxc_helpers::expression_is_reproducible).
//! `it.next().value && it.next().value` reads two different elements: the calls
//! advance the iterator between the two reads.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, expression_is_reproducible};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BinaryOperator;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// `x !== x` / `x != x` is the canonical NaN-detection idiom: `NaN` is the only
/// value not equal to itself, so an inequality of identical operands is a
/// deliberate test, not the always-trivial result every other operator produces.
/// ESLint's `no-self-compare` documents this same exception.
fn is_inequality_operator(op: BinaryOperator) -> bool {
    matches!(op, BinaryOperator::StrictInequality | BinaryOperator::Inequality)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression, AstType::LogicalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::BinaryExpression(bin) => {
                let op_str = match bin.operator {
                    BinaryOperator::StrictEquality => "===",
                    BinaryOperator::StrictInequality => "!==",
                    BinaryOperator::Subtraction => "-",
                    BinaryOperator::Division => "/",
                    _ => return,
                };

                let left_text = &ctx.source[bin.left.span().start as usize..bin.left.span().end as usize];
                let right_text = &ctx.source[bin.right.span().start as usize..bin.right.span().end as usize];

                // Avoid false positives on single-char tokens for `-` and `/`.
                if (op_str == "-" || op_str == "/") && left_text.len() <= 1 {
                    return;
                }

                if left_text != right_text {
                    return;
                }

                // Exempt the NaN-detection idiom `x !== x` / `x != x`.
                if is_inequality_operator(bin.operator) {
                    return;
                }

                if !expression_is_reproducible(&bin.left) || !expression_is_reproducible(&bin.right) {
                    return;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, bin.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Identical expression `{left_text}` on both sides of `{op_str}`."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
            AstKind::LogicalExpression(logical) => {
                let op_str = match logical.operator {
                    oxc_ast::ast::LogicalOperator::And => "&&",
                    oxc_ast::ast::LogicalOperator::Or => "||",
                    _ => return,
                };

                let left_text = &ctx.source
                    [logical.left.span().start as usize..logical.left.span().end as usize];
                let right_text = &ctx.source
                    [logical.right.span().start as usize..logical.right.span().end as usize];

                if left_text != right_text {
                    return;
                }

                if !expression_is_reproducible(&logical.left) || !expression_is_reproducible(&logical.right) {
                    return;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, logical.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Identical expression `{left_text}` on both sides of `{op_str}`."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
            _ => {}
        }
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // Regression for #1894: `x !== x` is the canonical NaN-detection idiom, not
    // a bug. `NaN` is the only value not equal to itself.
    #[test]
    fn allows_strict_inequality_nan_idiom() {
        let src = r#"export const isNaN = (obj: any): boolean => obj !== obj;"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_loose_inequality_nan_idiom() {
        assert!(run(r#"const isNaN = (x: any) => x != x;"#).is_empty());
    }

    #[test]
    fn flags_strict_equality_self_compare() {
        assert_eq!(run(r#"const b = x === x;"#).len(), 1);
    }

    #[test]
    fn flags_subtraction_of_identical_operands() {
        assert_eq!(run(r#"const z = foo - foo;"#).len(), 1);
    }

    // Issue #6853: a call advances state, so two identical call texts are two
    // distinct evaluations, not a duplicated operand.
    #[test]
    fn allows_repeated_iterator_next_calls() {
        let src = r#"const ok = it.next().done && it.next().done;"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_repeated_calls_around_subtraction() {
        let src = r#"const delta = queue.shift() - queue.shift();"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_repeated_calls_around_strict_equality() {
        let src = r#"const same = read() === read();"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // `it.done` is the member access of `it.next().done` without the call, and
    // it is still reported — proof that the silence above is not vacuous.
    #[test]
    fn flags_identical_call_free_member_access() {
        assert_eq!(run(r#"const ok = it.done && it.done;"#).len(), 1);
    }

    #[test]
    fn flags_identical_computed_member_access() {
        assert_eq!(run(r#"const z = xs[i] - xs[i];"#).len(), 1);
    }

    #[test]
    fn flags_identical_optional_chain_operands() {
        assert_eq!(run(r#"const ok = a?.b && a?.b;"#).len(), 1);
    }

    #[test]
    fn flags_identical_meta_property_operands() {
        assert_eq!(
            run(r#"const dev = import.meta.env.DEV && import.meta.env.DEV;"#).len(),
            1
        );
    }

    // Spreading an iterable drains it, so the second read sees a different
    // sequence — the array literal around it does not make the operand stable.
    #[test]
    fn allows_spread_array_operands() {
        let src = r#"const ok = [...gen].length && [...gen].length;"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }
}
