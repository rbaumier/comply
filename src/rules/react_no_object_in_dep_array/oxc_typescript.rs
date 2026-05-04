//! OxcCheck backend for react-no-object-in-dep-array.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, ArrayExpressionElement, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const HOOKS: &[&str] = &["useEffect", "useMemo", "useCallback"];

const ALLOCATING_CONSTRUCTORS: &[&str] = &[
    "Map", "Set", "WeakMap", "WeakSet", "Date", "Error", "Array", "Object", "RegExp", "Promise",
];

const ALLOCATING_MEMBER_CALLS: &[(&str, &str)] = &[("Object", "assign"), ("Object", "create")];

fn hook_name<'a>(call: &'a oxc_ast::ast::CallExpression<'a>) -> Option<&'a str> {
    match &call.callee {
        Expression::Identifier(id) => {
            let name = id.name.as_str();
            if HOOKS.contains(&name) {
                Some(name)
            } else {
                None
            }
        }
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                if obj.name.as_str() == "React" {
                    let name = member.property.name.as_str();
                    if HOOKS.contains(&name) {
                        return Some(name);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn label_for_element(elem: &ArrayExpressionElement) -> Option<(String, oxc_span::Span)> {
    match elem {
        ArrayExpressionElement::ObjectExpression(obj) => {
            Some(("Object literal".to_string(), obj.span))
        }
        ArrayExpressionElement::ArrayExpression(arr) => {
            Some(("Array literal".to_string(), arr.span))
        }
        ArrayExpressionElement::ArrowFunctionExpression(arrow) => {
            Some(("Inline arrow function".to_string(), arrow.span))
        }
        ArrayExpressionElement::FunctionExpression(func) => {
            Some(("Inline function expression".to_string(), func.span()))
        }
        ArrayExpressionElement::NewExpression(new_expr) => {
            let Expression::Identifier(ctor) = &new_expr.callee else {
                return None;
            };
            let name = ctor.name.as_str();
            if ALLOCATING_CONSTRUCTORS.contains(&name) {
                Some((format!("`new {name}()`"), new_expr.span))
            } else {
                None
            }
        }
        ArrayExpressionElement::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return None;
            };
            let Expression::Identifier(obj) = &member.object else {
                return None;
            };
            let obj_name = obj.name.as_str();
            let prop_name = member.property.name.as_str();
            if ALLOCATING_MEMBER_CALLS.contains(&(obj_name, prop_name)) {
                Some((format!("`{obj_name}.{prop_name}()` call"), call.span))
            } else {
                None
            }
        }
        ArrayExpressionElement::SpreadElement(spread) => {
            match &spread.argument {
                Expression::ObjectExpression(_) => {
                    Some(("Spread of object literal".to_string(), spread.span))
                }
                Expression::ArrayExpression(_) => {
                    Some(("Spread of array literal".to_string(), spread.span))
                }
                _ => None,
            }
        }
        _ => None,
    }
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
        let Some(fn_name) = hook_name(call) else {
            return;
        };
        if call.arguments.len() < 2 {
            return;
        }
        let Some(last_arg) = call.arguments.last() else {
            return;
        };
        let Argument::ArrayExpression(deps) = last_arg else {
            return;
        };

        for dep in &deps.elements {
            let Some((label, span)) = label_for_element(dep) else {
                continue;
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "{label} in `{fn_name}` dep array — creates a fresh reference \
                     every render. Extract to a memoized value or depend on \
                     primitive fields instead."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
