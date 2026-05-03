//! playwright-prefer-to-have-count OxcCheck backend.
//!
//! Flag `expect(await locator.count()).toBe(n)` patterns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

const EQUALITY_MATCHERS: &[&str] = &["toBe", "toEqual", "toStrictEqual"];

pub struct Check;

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
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `<expect-call>.toBe` / `.toEqual` / `.toStrictEqual`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let matcher = member.property.name.as_str();
        if !EQUALITY_MATCHERS.contains(&matcher) {
            return;
        }

        // Object side must be an `expect(...)` call.
        let Expression::CallExpression(obj_call) = &member.object else {
            return;
        };
        let Expression::Identifier(obj_fn) = &obj_call.callee else {
            return;
        };
        if obj_fn.name.as_str() != "expect" {
            return;
        }

        // First argument of expect must be `await <locator>.count()`.
        let Some(Argument::AwaitExpression(await_expr)) = obj_call.arguments.first() else {
            return;
        };
        let Expression::CallExpression(inner_call) = &await_expr.argument else {
            return;
        };
        let Expression::StaticMemberExpression(inner_member) = &inner_call.callee else {
            return;
        };
        if inner_member.property.name.as_str() != "count" {
            return;
        }
        if !inner_call.arguments.is_empty() {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, obj_fn.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `expect(locator).toHaveCount(n)` instead of `expect(await locator.count()).toBe(n)`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
