//! express-session-require-name oxc backend — flag `session({ ... })` calls
//! whose config object is missing the `name` property.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind};
use std::sync::Arc;

pub struct Check;

fn object_has_property(obj: &oxc_ast::ast::ObjectExpression, key: &str) -> bool {
    obj.properties.iter().any(|prop| {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { return false };
        p.key.static_name().is_some_and(|n| n == key)
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["session"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::Identifier(callee) = &call.callee else { return };
        if callee.name.as_str() != "session" {
            return;
        }

        // First argument must be an object literal.
        let Some(arg) = call.arguments.first() else { return };
        let Some(Expression::ObjectExpression(obj)) = arg.as_expression() else { return };

        if object_has_property(obj, "name") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "session config is missing `name` \u{2014} add a custom cookie name so the default `connect.sid` doesn't leak the server stack.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
