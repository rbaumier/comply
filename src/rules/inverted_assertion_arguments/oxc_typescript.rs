//! inverted-assertion-arguments oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

fn is_literal_expr(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::NumericLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
    )
}

fn is_variable_expr(expr: &Expression) -> bool {
    matches!(expr, Expression::Identifier(_))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["toBe", "toEqual"])
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

        // Only test files.
        let path_str = ctx.path.to_string_lossy();
        if !path_str.contains(".test.")
            && !path_str.contains(".spec.")
            && !path_str.contains("__tests__")
            && !path_str.contains("_test.")
        {
            return;
        }

        // Callee must be `<obj>.toBe(...)` or `<obj>.toEqual(...)`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if method != "toBe" && method != "toEqual" {
            return;
        }

        // Matcher argument must be a variable.
        if call.arguments.len() != 1 {
            return;
        }
        let matcher_arg = match &call.arguments[0] {
            Argument::Identifier(id) => {
                let _ = id;
                true
            }
            _ => {
                if let Some(expr) = call.arguments[0].as_expression() {
                    is_variable_expr(expr)
                } else {
                    false
                }
            }
        };
        if !matcher_arg {
            return;
        }

        // Object must be `expect(...)`.
        let Expression::CallExpression(expect_call) = &member.object else {
            return;
        };
        let Expression::Identifier(expect_id) = &expect_call.callee else {
            return;
        };
        if expect_id.name.as_str() != "expect" {
            return;
        }

        // Expect argument must be a literal.
        if expect_call.arguments.len() != 1 {
            return;
        }
        let expect_arg_is_literal = if let Some(expr) = expect_call.arguments[0].as_expression() {
            is_literal_expr(expr)
        } else {
            false
        };
        if !expect_arg_is_literal {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Expected and actual are inverted — put the literal in `.toBe()`/`.toEqual()`, not in `expect()`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn ignores_non_test_files() {
        // run_on uses "t.ts" (not a test file).
        assert!(run_on(r#"expect(42).toBe(result);"#).is_empty());
    }
}
