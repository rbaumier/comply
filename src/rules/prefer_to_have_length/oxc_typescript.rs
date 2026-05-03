//! prefer-to-have-length OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const LENGTH_MATCHERS: &[&str] = &["toBe", "toEqual"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["expect"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Outer: expect(x.length).<matcher>(n)
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let matcher = member.property.name.as_str();
        if !LENGTH_MATCHERS.contains(&matcher) {
            return;
        }

        // The object should be `expect(x.length)`.
        let Expression::CallExpression(expect_call) = &member.object else { return };
        let Expression::Identifier(expect_fn) = &expect_call.callee else { return };
        if expect_fn.name.as_str() != "expect" {
            return;
        }

        // Argument to expect(...) should be `<something>.length`.
        let Some(first_arg) = expect_call.arguments.first() else { return };
        let Some(Expression::StaticMemberExpression(arg_member)) = first_arg.as_expression()
        else {
            return;
        };
        if arg_member.property.name.as_str() != "length" {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Use `toHaveLength(n)` instead of `expect(x.length).{matcher}(n)`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
