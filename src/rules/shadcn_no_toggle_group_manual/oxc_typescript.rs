//! OxcCheck backend for shadcn-no-toggle-group-manual.
//!
//! Detect `.map(... => <Button variant={cond ? X : Y}>...</Button>)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue,
    JSXElementName, JSXExpression, Statement,
};
use oxc_span::GetSpan;
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

        // Callee must be `*.map`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "map" {
            return;
        }

        for arg in &call.arguments {
            let Argument::ArrowFunctionExpression(arrow) = arg else {
                if let Argument::FunctionExpression(func) = arg {
                    if let Some(body) = &func.body {
                        if let Some(jsx) = find_returned_jsx_in_stmts(&body.statements) {
                            if is_button_with_ternary_variant(jsx) {
                                let (line, column) =
                                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line,
                                    column,
                                    rule_id: super::META.id.into(),
                                    message: "Manual toggle group — replace `.map(... => <Button variant={cond ? ... : ...}>)` with `<ToggleGroup>` + `<ToggleGroupItem>`.".into(),
                                    severity: Severity::Warning,
                                    span: None,
                                });
                                return;
                            }
                        }
                    }
                }
                continue;
            };

            let jsx = if arrow.expression {
                jsx_from_expr(&arrow.body)
            } else {
                find_returned_jsx_in_stmts(&arrow.body.statements)
            };

            if let Some(jsx) = jsx {
                if is_button_with_ternary_variant(jsx) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Manual toggle group — replace `.map(... => <Button variant={cond ? ... : ...}>)` with `<ToggleGroup>` + `<ToggleGroupItem>`.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
            }
        }
    }
}

enum JsxRef<'a> {
    Element(&'a oxc_ast::ast::JSXElement<'a>),
    // Self-closing elements in OXC are JSXElements with self_closing=true on the opening element
}

fn jsx_from_expr<'a>(expr: &'a oxc_ast::ast::FunctionBody<'a>) -> Option<JsxRef<'a>> {
    // For expression-body arrows, the body statements contain a single ExpressionStatement
    for stmt in &expr.statements {
        if let Statement::ExpressionStatement(es) = stmt {
            return jsx_from_expression(&es.expression);
        }
    }
    None
}

fn jsx_from_expression<'a>(expr: &'a Expression<'a>) -> Option<JsxRef<'a>> {
    match expr {
        Expression::JSXElement(el) => Some(JsxRef::Element(el)),
        Expression::ParenthesizedExpression(paren) => jsx_from_expression(&paren.expression),
        _ => None,
    }
}

fn find_returned_jsx_in_stmts<'a>(
    stmts: &'a [Statement<'a>],
) -> Option<JsxRef<'a>> {
    for stmt in stmts {
        if let Statement::ReturnStatement(ret) = stmt {
            if let Some(arg) = &ret.argument {
                return jsx_from_expression(arg);
            }
        }
    }
    None
}

fn is_button_with_ternary_variant(jsx: JsxRef) -> bool {
    let JsxRef::Element(el) = jsx;
    let Some(tag) = jsx_tag_name(&el.opening_element.name) else {
        return false;
    };
    if tag != "Button" {
        return false;
    }
    for attr_item in &el.opening_element.attributes {
        let JSXAttributeItem::Attribute(attr) = attr_item else {
            continue;
        };
        let JSXAttributeName::Identifier(name) = &attr.name else {
            continue;
        };
        if name.name.as_str() != "variant" {
            continue;
        }
        let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
            continue;
        };
        if let JSXExpression::ConditionalExpression(_) = &container.expression {
            return true;
        }
    }
    false
}

fn jsx_tag_name<'a>(name: &'a JSXElementName<'a>) -> Option<&'a str> {
    match name {
        JSXElementName::Identifier(id) => Some(id.name.as_str()),
        JSXElementName::IdentifierReference(id) => Some(id.name.as_str()),
        _ => None,
    }
}
