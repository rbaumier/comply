//! OxcCheck backend — flag `test("... and ...", ...)` / `it("... and ...", ...)`
//! names that combine multiple behaviors.

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
        let path = ctx.path.to_string_lossy();
        if !path.contains(".test.") && !path.contains(".spec.") {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else { return };
        let callee_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if callee_name != "test" && callee_name != "it" {
            return;
        }
        let Some(first_arg) = call.arguments.first() else { return };
        let (unquoted, span_start) = match first_arg {
            Argument::StringLiteral(s) => (s.value.as_str(), s.span.start as usize),
            Argument::TemplateLiteral(t) => {
                // Only check simple template literals (no expressions)
                if !t.expressions.is_empty() || t.quasis.len() != 1 {
                    return;
                }
                (t.quasis[0].value.raw.as_str(), t.span.start as usize)
            }
            _ => return,
        };
        if !unquoted.contains(" and ") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Test name {unquoted:?} contains \" and \" — split into two focused tests."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(s, &Check, "foo.test.ts")
    }


    #[test]
    fn flags_and_in_test_name() {
        assert_eq!(
            run("test('validates email and sends confirmation', () => {})").len(),
            1
        );
    }


    #[test]
    fn allows_single_behavior() {
        assert!(run("test('validates email format', () => {})").is_empty());
    }
}
