//! OxcCheck backend for prefer-logical-operator-over-ternary.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::UnaryOperator;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Compare two source text snippets after trimming whitespace.
fn same_text(a: &str, b: &str) -> bool {
    let a = a.trim();
    let b = b.trim();
    !a.is_empty() && a == b
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ConditionalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ConditionalExpression(cond) = node.kind() else { return };

        let test_text = &ctx.source[cond.test.span().start as usize..cond.test.span().end as usize];
        let consequent_text = &ctx.source[cond.consequent.span().start as usize..cond.consequent.span().end as usize];
        let alternate_text = &ctx.source[cond.alternate.span().start as usize..cond.alternate.span().end as usize];

        // Pattern 1: `foo ? foo : bar` — test === consequent
        if same_text(test_text, consequent_text) {
            let (line, column) = byte_offset_to_line_col(ctx.source, cond.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Prefer `{test_text} || {alternate_text}` (or `??`) over `{test_text} ? {test_text} : {alternate_text}`."
                ),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // Pattern 2: `!bar ? foo : bar` — negated test.argument === alternate
        if let oxc_ast::ast::Expression::UnaryExpression(unary) = &cond.test
            && unary.operator == UnaryOperator::LogicalNot {
                let arg_text = &ctx.source[unary.argument.span().start as usize..unary.argument.span().end as usize];
                if same_text(arg_text, alternate_text) {
                    let (line, column) = byte_offset_to_line_col(ctx.source, cond.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Prefer `{alternate_text} || {consequent_text}` (or `??`) over \
                             `!{alternate_text} ? {consequent_text} : {alternate_text}`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_test_equals_consequent() {
        let d = run_on("const x = foo ? foo : bar;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("||"));
    }


    #[test]
    fn flags_negated_test_equals_alternate() {
        let d = run_on("const x = !bar ? foo : bar;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("||"));
    }


    #[test]
    fn allows_distinct_arms() {
        assert!(run_on("const x = a ? b : c;").is_empty());
    }


    #[test]
    fn allows_test_equals_alternate_no_negation() {
        // `foo ? bar : foo` — not a simple || pattern
        assert!(run_on("const x = foo ? bar : foo;").is_empty());
    }


    #[test]
    fn flags_member_expression() {
        let d = run_on("const x = a.b ? a.b : c;");
        assert_eq!(d.len(), 1);
    }
}
