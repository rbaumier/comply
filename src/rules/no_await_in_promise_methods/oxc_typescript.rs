//! no-await-in-promise-methods OxcCheck backend — flag `await` inside
//! `Promise.all/race/any/allSettled` arrays.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const PROMISE_METHODS: &[&str] = &["all", "allSettled", "any", "race"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Promise"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be `Promise.<method>`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name != "Promise" {
            return;
        }
        let method_name = member.property.name.as_str();
        if !PROMISE_METHODS.contains(&method_name) {
            return;
        }

        // First argument must be an array
        let Some(first_arg) = call.arguments.first() else { return };
        if call.arguments.len() != 1 {
            return;
        }
        let oxc_ast::ast::Argument::ArrayExpression(arr) = first_arg else { return };

        // Walk array elements looking for AwaitExpression
        for element in &arr.elements {
            let oxc_ast::ast::ArrayExpressionElement::AwaitExpression(await_expr) = element else {
                continue;
            };
            let (line, column) =
                byte_offset_to_line_col(ctx.source, await_expr.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Promise in `Promise.{method_name}()` should not be awaited \
                     — this serializes the calls."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
