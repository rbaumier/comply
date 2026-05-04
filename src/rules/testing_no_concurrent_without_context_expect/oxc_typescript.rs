//! OXC backend for testing-no-concurrent-without-context-expect.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
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

        // Callee must be `test.concurrent` or `it.concurrent`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "concurrent" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if !matches!(obj.name.as_str(), "test" | "it") {
            return;
        }

        // Find the callback argument (arrow function or function expression).
        let Some(callback) = call.arguments.iter().find_map(|arg| {
            let expr = arg.as_expression()?;
            match expr {
                Expression::ArrowFunctionExpression(f) => Some(&f.params),
                Expression::FunctionExpression(f) => Some(&f.params),
                _ => None,
            }
        }) else {
            return;
        };

        // Check if the first parameter destructures `expect`.
        let has_expect = callback.items.first().is_some_and(|param| {
            if let BindingPattern::ObjectPattern(obj_pat) = &param.pattern {
                obj_pat.properties.iter().any(|prop| {
                    let key_name = prop.key.name();
                    key_name.as_deref() == Some("expect")
                })
            } else {
                false
            }
        });

        if !has_expect {
            let span = callback.span;
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "test.concurrent must destructure { expect } from the test context — the module-level expect is not scoped per concurrent test.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
