//! ts-prefer-promise-reject-errors OXC backend — flag `Promise.reject(<literal>)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

fn is_non_error_argument(arg: &Argument) -> bool {
    match arg {
        Argument::StringLiteral(_)
        | Argument::TemplateLiteral(_)
        | Argument::NumericLiteral(_)
        | Argument::BooleanLiteral(_)
        | Argument::NullLiteral(_)
        | Argument::ObjectExpression(_)
        | Argument::ArrayExpression(_)
        | Argument::RegExpLiteral(_) => true,
        Argument::Identifier(id) => id.name.as_str() == "undefined",
        Argument::ParenthesizedExpression(paren) => {
            // Unwrap parenthesized: check inner expression
            is_non_error_expr(&paren.expression)
        }
        _ => false,
    }
}

fn is_non_error_expr(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_)
        | Expression::TemplateLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::ObjectExpression(_)
        | Expression::ArrayExpression(_)
        | Expression::RegExpLiteral(_) => true,
        Expression::Identifier(id) => id.name.as_str() == "undefined",
        Expression::ParenthesizedExpression(paren) => is_non_error_expr(&paren.expression),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Promise.reject"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "Promise" {
            return;
        }
        if member.property.name.as_str() != "reject" {
            return;
        }

        let Some(first) = call.arguments.first() else {
            return;
        };
        if !is_non_error_argument(first) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`Promise.reject()` should be called with an `Error` instance, \
                      not a primitive or object literal."
                .into(),
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
    fn flags_reject_string() {
        let d = run_on("Promise.reject('boom');");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_reject_number() {
        let d = run_on("Promise.reject(42);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_reject_object_literal() {
        let d = run_on("Promise.reject({ code: 500 });");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_reject_template_string() {
        let d = run_on("Promise.reject(`boom ${x}`);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_reject_new_error() {
        assert!(run_on("Promise.reject(new Error('boom'));").is_empty());
    }


    #[test]
    fn allows_reject_identifier() {
        assert!(run_on("Promise.reject(err);").is_empty());
    }


    #[test]
    fn allows_reject_call() {
        assert!(run_on("Promise.reject(makeError());").is_empty());
    }


    #[test]
    fn allows_promise_resolve() {
        assert!(run_on("Promise.resolve('value');").is_empty());
    }
}
