//! rn-push-token-requires-projectid OXC backend — flag `getExpoPushTokenAsync()`
//! calls whose argument object lacks `projectId`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

fn callee_ends_with(callee: &Expression, name: &str) -> bool {
    match callee {
        Expression::Identifier(id) => id.name.as_str() == name,
        Expression::StaticMemberExpression(member) => member.property.name.as_str() == name,
        _ => false,
    }
}

fn object_has_project_id(expr: &Expression) -> bool {
    let Expression::ObjectExpression(obj) = expr else {
        return false;
    };
    obj.properties.iter().any(|prop| {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            return false;
        };
        match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str() == "projectId",
            PropertyKey::StringLiteral(s) => s.value.as_str() == "projectId",
            _ => false,
        }
    })
}

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
        let AstKind::CallExpression(call) = node.kind() else { return };

        if !callee_ends_with(&call.callee, "getExpoPushTokenAsync") {
            return;
        }

        let has_project_id = call
            .arguments
            .first()
            .and_then(|arg| arg.as_expression())
            .is_some_and(|e| object_has_project_id(e));

        if has_project_id {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`getExpoPushTokenAsync` must be called with `{ projectId }` — required by EAS.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
