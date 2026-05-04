//! no-invalid-remove-event-listener OXC backend — flag `removeEventListener`
//! whose listener argument is an inline function or `.bind()` call.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

fn is_inline_listener(arg: &Argument) -> bool {
    match arg {
        Argument::ArrowFunctionExpression(_) | Argument::FunctionExpression(_) => true,
        Argument::CallExpression(call) => {
            // Check for `.bind(...)` call
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            member.property.name.as_str() == "bind"
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["removeEventListener"])
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

        // Callee must be `<x>.removeEventListener`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "removeEventListener" {
            return;
        }

        // Second argument (the listener) must be inline
        let Some(listener) = call.arguments.get(1) else {
            return;
        };
        if !is_inline_listener(listener) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "The listener argument should be a function reference — inline functions and `.bind()` create a new reference each call.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
