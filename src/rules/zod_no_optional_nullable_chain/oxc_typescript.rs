//! OXC backend for zod-no-optional-nullable-chain.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use oxc_ast::ast::Expression;

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

        // Outer call must be `.optional()` or `.nullable()`.
        let Some((method, object)) = static_method_call(&call.callee) else {
            return;
        };
        if method != "optional" && method != "nullable" {
            return;
        }

        let other = if method == "optional" {
            "nullable"
        } else {
            "optional"
        };

        // The object must itself be a call to the complementary method.
        let Expression::CallExpression(inner_call) = object else {
            return;
        };
        let Some((inner_method, _)) = static_method_call(&inner_call.callee) else {
            return;
        };
        if inner_method != other {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Replace `.optional().nullable()` with `.nullish()` for clearer intent."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// If `expr` is a `obj.method` static member expression, return `(method_name, &object)`.
fn static_method_call<'a>(expr: &'a Expression<'a>) -> Option<(&'a str, &'a Expression<'a>)> {
    let Expression::StaticMemberExpression(mem) = expr else {
        return None;
    };
    Some((mem.property.name.as_str(), &mem.object))
}
