//! OxcCheck backend for react-no-hydration-flicker.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement};
use std::sync::Arc;

pub struct Check;

fn is_effect_hook(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => {
            matches!(id.name.as_str(), "useEffect" | "useLayoutEffect")
        }
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                obj.name.as_str() == "React"
                    && matches!(
                        member.property.name.as_str(),
                        "useEffect" | "useLayoutEffect"
                    )
            } else {
                false
            }
        }
        _ => false,
    }
}

fn has_empty_deps_array(call: &oxc_ast::ast::CallExpression) -> bool {
    if call.arguments.len() < 2 {
        return false;
    }
    let Argument::ArrayExpression(arr) = &call.arguments[1] else {
        return false;
    };
    arr.elements.is_empty()
}

const NON_SETTER_SET_FNS: &[&str] = &["setTimeout", "setInterval", "setImmediate"];

fn is_setter_call_name(name: &str) -> bool {
    if NON_SETTER_SET_FNS.contains(&name) {
        return false;
    }
    if let Some(rest) = name.strip_prefix("set") {
        rest.starts_with(|c: char| c.is_ascii_uppercase())
    } else {
        false
    }
}

fn is_setter_call_expr(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::Identifier(id) = &call.callee else {
        return false;
    };
    is_setter_call_name(id.name.as_str())
}

fn body_is_sole_setter(body: &oxc_ast::ast::FunctionBody, is_expression: bool) -> bool {
    if is_expression {
        // Concise arrow: `() => setVal(true)`
        if body.statements.len() != 1 {
            return false;
        }
        let Statement::ExpressionStatement(es) = &body.statements[0] else {
            return false;
        };
        return is_setter_call_expr(&es.expression);
    }

    // Block body: single expression statement
    if body.statements.len() != 1 {
        return false;
    }
    let Statement::ExpressionStatement(es) = &body.statements[0] else {
        return false;
    };
    is_setter_call_expr(&es.expression)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useEffect", "useLayoutEffect"])
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
        if !is_effect_hook(call) {
            return;
        }
        if !has_empty_deps_array(call) {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let sole_setter = match first_arg {
            Argument::ArrowFunctionExpression(arrow) => {
                body_is_sole_setter(&arrow.body, arrow.expression)
            }
            Argument::FunctionExpression(func) => {
                if let Some(body) = &func.body {
                    body_is_sole_setter(body, false)
                } else {
                    false
                }
            }
            _ => false,
        };

        if !sole_setter {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`useEffect(setState, [])` on mount causes a hydration flash — use `useSyncExternalStore` or `suppressHydrationWarning`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
