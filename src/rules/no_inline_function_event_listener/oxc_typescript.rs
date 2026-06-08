//! no-inline-function-event-listener oxc backend.

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

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["addEventListener"])
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
        if member.property.name.as_str() != "addEventListener" {
            return;
        }

        // Check if the second argument is an inline function.
        let Some(second) = call.arguments.get(1) else {
            return;
        };
        if !matches!(
            second,
            Argument::ArrowFunctionExpression(_) | Argument::FunctionExpression(_)
        ) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Inline function passed to addEventListener cannot be removed — extract to a named function for proper cleanup.".into(),
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
    fn flags_inline_arrow() {
        assert_eq!(
            run_on("el.addEventListener('click', () => doThing())").len(),
            1
        );
    }


    #[test]
    fn flags_inline_function_expression() {
        assert_eq!(
            run_on("el.addEventListener('click', function () { doThing(); })").len(),
            1
        );
    }


    #[test]
    fn allows_named_identifier_reference() {
        assert!(run_on("el.addEventListener('click', handleClick)").is_empty());
    }


    #[test]
    fn allows_member_expression_reference() {
        assert!(run_on("el.addEventListener('click', this.handleClick)").is_empty());
    }


    #[test]
    fn ignores_non_addeventlistener_calls() {
        assert!(run_on("arr.forEach(() => doThing())").is_empty());
    }
}
