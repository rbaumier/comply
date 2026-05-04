//! audit-log-required-fields OXC backend — flag audit-logging calls whose
//! payload is missing `userId`, `timestamp`, or `action`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind};
use std::sync::Arc;

pub struct Check;

const AUDIT_FN_NAMES: &[&str] = &["auditLog", "audit"];

const REQUIRED_KEYS: &[&[&str]] = &[
    &["userId", "user_id", "actorId", "actor_id"],
    &["timestamp", "ts", "createdAt", "created_at", "at", "time"],
    &["action", "event", "type", "verb"],
];

fn is_audit_call(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(ident) => AUDIT_FN_NAMES.contains(&ident.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            let method = member.property.name.as_str();
            if AUDIT_FN_NAMES.contains(&method) {
                return true;
            }
            // `audit.log(...)` / `*audit*.log(...)`
            if method == "log"
                && let Expression::Identifier(obj) = &member.object {
                    return obj.name.as_str().contains("audit");
                }
            false
        }
        _ => false,
    }
}

fn object_has_any_of(obj: &oxc_ast::ast::ObjectExpression, keys: &[&str]) -> bool {
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        let key_name = match &p.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if p.shorthand {
            // Shorthand property: `{ userId }` — key is the identifier name.
            if keys.contains(&key_name) {
                return true;
            }
        } else if keys.contains(&key_name) {
            return true;
        }
    }
    false
}

fn object_missing_required(obj: &oxc_ast::ast::ObjectExpression) -> Option<&'static str> {
    for group in REQUIRED_KEYS {
        if !object_has_any_of(obj, group) {
            return Some(group[0]);
        }
    }
    None
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        if !is_audit_call(&call.callee) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);

        let Some(first_arg) = call.arguments.first() else {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Audit log call is missing required fields (`userId`, `timestamp`, `action`).".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        };

        let Some(Expression::ObjectExpression(obj)) = first_arg.as_expression() else {
            return;
        };

        if let Some(missing) = object_missing_required(obj) {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Audit log entry is missing required field `{missing}` (or equivalent)."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
