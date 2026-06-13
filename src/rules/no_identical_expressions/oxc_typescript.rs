//! no-identical-expressions oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
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

                // Exempt the NaN-detection idiom `x !== x` / `x != x`.
                if left_text == right_text && !is_inequality_operator(bin.operator) {
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

                if left_text == right_text {
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
}
