//! no-gratuitous-expression OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, LogicalOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn detect_self_comparison(op: BinaryOperator, left: &Expression, right: &Expression, source: &str) -> Option<&'static str> {
    // Both sides must be identifiers (or member expressions) with the same text
    let left_span = left.span();
    let right_span = right.span();
    let left_text = &source[left_span.start as usize..left_span.end as usize];
    let right_text = &source[right_span.start as usize..right_span.end as usize];

    let left_trimmed = left_text.trim();
    let right_trimmed = right_text.trim();

    if left_trimmed.is_empty() || left_trimmed != right_trimmed {
        return None;
    }
    if !left_trimmed.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
        return None;
    }

    match op {
        // `x !== x` / `x != x` is the canonical NaN-detection idiom: `NaN` is the
        // only value not equal to itself, so an inequality of identical operands is
        // a deliberate test, not a dead branch. ESLint's `no-self-compare`
        // documents this same exception. Equality self-comparison stays flagged —
        // `x === x` is genuinely always true.
        BinaryOperator::StrictInequality | BinaryOperator::Inequality => None,
        BinaryOperator::StrictEquality | BinaryOperator::Equality => {
            Some("comparison `x === x` is always true (unless NaN)")
        }
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement, AstType::BinaryExpression, AstType::LogicalExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["true", "false"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::IfStatement(if_stmt) => {
                if let Expression::BooleanLiteral(lit) = &if_stmt.test {
                    let msg = if lit.value {
                        "Gratuitous expression: condition is always true."
                    } else {
                        "Gratuitous expression: condition is always false."
                    };
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, if_stmt.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: msg.into(),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            AstKind::LogicalExpression(logical) => {
                // `&& false` → always false; `|| true` → always true
                match logical.operator {
                    LogicalOperator::And => {
                        if let Expression::BooleanLiteral(lit) = &logical.right
                            && !lit.value {
                                let (line, column) =
                                    byte_offset_to_line_col(ctx.source, logical.span.start as usize);
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line,
                                    column,
                                    rule_id: super::META.id.into(),
                                    message: "Gratuitous expression: expression is always false (short-circuited by `&& false`).".into(),
                                    severity: Severity::Error,
                                    span: None,
                                });
                            }
                    }
                    LogicalOperator::Or => {
                        if let Expression::BooleanLiteral(lit) = &logical.right
                            && lit.value {
                                let (line, column) =
                                    byte_offset_to_line_col(ctx.source, logical.span.start as usize);
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line,
                                    column,
                                    rule_id: super::META.id.into(),
                                    message: "Gratuitous expression: expression is always true (short-circuited by `|| true`).".into(),
                                    severity: Severity::Error,
                                    span: None,
                                });
                            }
                    }
                    _ => {}
                }
            }
            AstKind::BinaryExpression(bin) => {
                if let Some(message) = detect_self_comparison(bin.operator, &bin.left, &bin.right, ctx.source) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, bin.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!("Gratuitous expression: {}.", message),
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
    // a dead branch. `NaN` is the only value not equal to itself.
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
    fn flags_loose_equality_self_compare() {
        assert_eq!(run(r#"const b = x == x;"#).len(), 1);
    }
}
