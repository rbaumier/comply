//! OXC backend for no-mutating-assign — flag `Object.assign(target, ...)`
//! where `target` is not an empty object literal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Object.assign"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be `Object.assign`.
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name != "Object" || member.property.name != "assign" {
            return;
        }

        // Need at least one argument.
        let Some(first) = call.arguments.first() else { return };

        // Allow `Object.assign({}, ...)`.
        if let oxc_ast::ast::Argument::ObjectExpression(obj_expr) = first
            && obj_expr.properties.is_empty() {
                return;
            }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`Object.assign()` with a non-empty target mutates the target in place \
                      — use `{...target, ...source}` or `Object.assign({}, target, source)` instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
