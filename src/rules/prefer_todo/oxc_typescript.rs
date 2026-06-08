//! prefer-todo oxc backend — flag empty test bodies.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
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
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        let name = callee.name.as_str();
        if name != "test" && name != "it" {
            return;
        }

        // Second argument should be a function/arrow with an empty body.
        let Some(second) = call.arguments.get(1) else {
            return;
        };

        let body_is_empty = match second {
            Argument::ArrowFunctionExpression(arrow) => {
                arrow.body.statements.is_empty()
            }
            Argument::FunctionExpression(func) => {
                func.body.as_ref().is_some_and(|b| b.statements.is_empty())
            }
            _ => false,
        };

        if !body_is_empty {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Empty `{name}` body — use `{name}.todo('...')` to mark this as a \
                 placeholder so the runner reports it as pending.",
            ),
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
    fn flags_empty_test_arrow() {
        assert_eq!(run_on("test('x', () => {});").len(), 1);
    }


    #[test]
    fn flags_empty_it_function() {
        assert_eq!(run_on("it('x', function () {});").len(), 1);
    }


    #[test]
    fn allows_test_todo() {
        assert!(run_on("test.todo('x');").is_empty());
    }


    #[test]
    fn allows_test_with_body() {
        assert!(run_on("test('x', () => { expect(1).toBe(1); });").is_empty());
    }


    #[test]
    fn ignores_non_test_calls() {
        assert!(run_on("foo('x', () => {});").is_empty());
    }
}
