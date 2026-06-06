//! prefer-object-from-entries OXC backend — flag `.reduce(…, {})` building objects.

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
        Some(&["reduce"])
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

        // Must be a `.reduce(` call.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "reduce" {
            return;
        }

        // Must have 2 arguments: callback and initial value.
        if call.arguments.len() != 2 {
            return;
        }

        // Check the second argument (initial value).
        let init = &call.arguments[1];
        let is_empty_object = match init {
            // `{}`
            Argument::ObjectExpression(obj) => obj.properties.is_empty(),
            // `Object.create(null)`
            Argument::CallExpression(inner_call) => {
                let Expression::StaticMemberExpression(m) = &inner_call.callee else {
                    return;
                };
                let Expression::Identifier(obj) = &m.object else {
                    return;
                };
                if obj.name.as_str() != "Object" || m.property.name.as_str() != "create" {
                    return;
                }
                inner_call.arguments.len() == 1
                    && matches!(
                        inner_call.arguments.first(),
                        Some(Argument::NullLiteral(_))
                    )
            }
            _ => false,
        };

        if !is_empty_object {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `Object.fromEntries()` over `Array#reduce()` to build an object."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
