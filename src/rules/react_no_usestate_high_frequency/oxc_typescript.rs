//! OXC backend for react-no-usestate-high-frequency.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
};
use oxc_span::GetSpan;
use std::sync::Arc;

const HIGH_FREQ_EVENTS: &[&str] = &["mousemove", "scroll", "resize", "pointermove", "wheel"];
const HIGH_FREQ_JSX_PROPS: &[&str] = &[
    "onMouseMove",
    "onScroll",
    "onPointerMove",
    "onWheel",
    "onDrag",
    "onDragOver",
    "onTouchMove",
];

pub struct Check;

fn handler_span_contains_setstate(
    handler_span: oxc_span::Span,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for n in semantic.nodes().iter() {
        let s = n.kind().span();
        if s.start < handler_span.start || s.end > handler_span.end {
            continue;
        }
        if let AstKind::CallExpression(call) = n.kind()
            && let Expression::Identifier(id) = &call.callee {
                let name = id.name.as_str();
                if name.starts_with("set")
                    && name.len() > 3
                    && name.as_bytes()[3].is_ascii_uppercase()
                {
                    return true;
                }
            }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::CallExpression(call) => {
                // Check for addEventListener("mousemove", handler)
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    return;
                };
                if member.property.name.as_str() != "addEventListener" {
                    return;
                }
                if call.arguments.len() < 2 {
                    return;
                }
                // First arg must be a string literal with a high-freq event
                let Some(ev_lit) = call.arguments[0].as_expression().and_then(|e| {
                    if let Expression::StringLiteral(s) = e { Some(s) } else { None }
                }) else {
                    return;
                };
                let ev = ev_lit.value.as_str();
                if !HIGH_FREQ_EVENTS.contains(&ev) {
                    return;
                }
                // Second arg is the handler
                let Some(handler_expr) = call.arguments[1].as_expression() else {
                    return;
                };
                let handler_span = match handler_expr {
                    Expression::ArrowFunctionExpression(arrow) => arrow.span,
                    Expression::FunctionExpression(func) => func.span,
                    _ => return,
                };
                if !handler_span_contains_setstate(handler_span, semantic) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`setState` inside a high-frequency event listener (mousemove/scroll/...) — \
                             use `useRef` for the transient value and only commit a render when needed."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::JSXOpeningElement(opening) => {
                for attr_item in &opening.attributes {
                    let JSXAttributeItem::Attribute(attr) = attr_item else {
                        continue;
                    };
                    let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                        continue;
                    };
                    let attr_name = name_ident.name.as_str();
                    if !HIGH_FREQ_JSX_PROPS.contains(&attr_name) {
                        continue;
                    }
                    let Some(JSXAttributeValue::ExpressionContainer(ec)) = &attr.value else {
                        continue;
                    };
                    let handler_span = match &ec.expression {
                        JSXExpression::ArrowFunctionExpression(arrow) => arrow.span,
                        JSXExpression::FunctionExpression(func) => func.span,
                        _ => continue,
                    };
                    if !handler_span_contains_setstate(handler_span, semantic) {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "`setState` inside a high-frequency JSX handler (onMouseMove/onScroll/...) — \
                                 use `useRef` for the transient value and only commit a render when needed."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
            }
            _ => {}
        }
    }
}
