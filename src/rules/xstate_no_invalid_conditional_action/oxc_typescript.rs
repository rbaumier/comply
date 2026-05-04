//! OXC backend for xstate-no-invalid-conditional-action.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

/// Check whether an ObjectExpression has a property with any of the given key names.
fn object_has_key(obj: &oxc_ast::ast::ObjectExpression, names: &[&str]) -> bool {
    obj.properties.iter().any(|prop| {
        if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop {
            if let Some(name) = p.key.name() {
                return names.contains(&name.as_ref());
            }
        }
        false
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `choose`.
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != "choose" {
            return;
        }

        // First argument should be an array literal of branch objects.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Argument::ArrayExpression(array) = first_arg else {
            return;
        };

        for element in &array.elements {
            let Some(expr) = element.as_expression() else {
                continue;
            };
            let Expression::ObjectExpression(obj) = expr else {
                continue;
            };

            let has_guard = object_has_key(obj, &["guard", "cond"]);
            let has_actions = object_has_key(obj, &["actions"]);

            if has_guard && has_actions {
                continue;
            }

            let message = match (has_guard, has_actions) {
                (false, false) => {
                    "`choose()` branch is missing both `guard`/`cond` and `actions`.".to_string()
                }
                (false, true) => "`choose()` branch is missing `guard`/`cond`.".to_string(),
                (true, false) => "`choose()` branch is missing `actions`.".to_string(),
                (true, true) => unreachable!(),
            };

            let (line, column) =
                byte_offset_to_line_col(ctx.source, obj.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message,
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
