use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_not_promise(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::ArrayExpression(_)
            | Expression::ArrowFunctionExpression(_)
            | Expression::BooleanLiteral(_)
            | Expression::ClassExpression(_)
            | Expression::FunctionExpression(_)
            | Expression::JSXElement(_)
            | Expression::JSXFragment(_)
            | Expression::NullLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::RegExpLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::TemplateLiteral(_)
            | Expression::UnaryExpression(_)
            | Expression::UpdateExpression(_)
            | Expression::BinaryExpression(_)
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AwaitExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AwaitExpression(await_expr) = node.kind() else { return };

        // Unwrap parenthesized expression layers.
        let mut unwrapped = &await_expr.argument;
        while let Expression::ParenthesizedExpression(paren) = unwrapped {
            unwrapped = &paren.expression;
        }

        // For sequence expressions, check the last expression.
        let check_expr = if let Expression::SequenceExpression(seq) = unwrapped {
            match seq.expressions.last() {
                Some(last) => last,
                None => return,
            }
        } else {
            unwrapped
        };

        if !is_not_promise(check_expr) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Do not `await` a non-promise value.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_await_number() {
        let d = run_on("async function f() { await 42; }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-unnecessary-await");
    }


    #[test]
    fn flags_await_string() {
        let d = run_on("async function f() { await 'hello'; }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_await_array() {
        let d = run_on("async function f() { await [1, 2, 3]; }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_await_arrow_function() {
        let d = run_on("async function f() { await (() => {}); }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_await_template_literal() {
        let d = run_on("async function f() { await `hello`; }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_await_unary() {
        let d = run_on("async function f() { await !true; }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_await_call() {
        assert!(run_on("async function f() { await fetch(url); }").is_empty());
    }


    #[test]
    fn allows_await_identifier() {
        assert!(run_on("async function f() { await promise; }").is_empty());
    }


    #[test]
    fn allows_await_new_promise() {
        assert!(run_on("async function f() { await new Promise(r => r()); }").is_empty());
    }
}
