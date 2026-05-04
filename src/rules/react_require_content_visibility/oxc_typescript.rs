//! OxcCheck backend for react-require-content-visibility.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, JSXElementName};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn in_jsx_expression<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()).skip(1) {
        if matches!(ancestor.kind(), AstKind::JSXExpressionContainer(_)) {
            return true;
        }
    }
    false
}

fn enclosing_virtualizer_tag<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()).skip(1) {
        if let AstKind::JSXOpeningElement(opening) = ancestor.kind() {
            let tag = match &opening.name {
                JSXElementName::Identifier(id) => id.name.as_str(),
                JSXElementName::IdentifierReference(id) => id.name.as_str(),
                _ => continue,
            };
            if tag.contains("Virtual")
                || tag.contains("Virtuoso")
                || tag.contains("Window")
                || tag.ends_with("List")
            {
                return true;
            }
        }
    }
    false
}

fn large_array_source(recv: &Expression, min_nodes: usize) -> bool {
    match recv {
        Expression::ArrayExpression(arr) => {
            let count = arr.elements.iter().count();
            count >= min_nodes
        }
        Expression::CallExpression(call) => {
            // `Array.from({ length: N })`
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            let Expression::Identifier(obj) = &member.object else {
                return false;
            };
            if obj.name.as_str() != "Array" || member.property.name.as_str() != "from" {
                return false;
            }
            let Some(Argument::ObjectExpression(obj_expr)) = call.arguments.first() else {
                return false;
            };
            for prop in &obj_expr.properties {
                if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop
                    && let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &p.key
                        && key.name.as_str() == "length"
                            && let Expression::NumericLiteral(n) = &p.value {
                                return (n.value as usize) >= min_nodes;
                            }
            }
            false
        }
        _ => false,
    }
}

fn is_known_small_array_source(recv: &Expression, min_nodes: usize) -> bool {
    match recv {
        Expression::ArrayExpression(arr) => {
            let count = arr.elements.iter().count();
            count < min_nodes
        }
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            let Expression::Identifier(obj) = &member.object else {
                return false;
            };
            if obj.name.as_str() != "Array" || member.property.name.as_str() != "from" {
                return false;
            }
            let Some(Argument::ObjectExpression(obj_expr)) = call.arguments.first() else {
                return false;
            };
            for prop in &obj_expr.properties {
                if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop
                    && let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &p.key
                        && key.name.as_str() == "length"
                            && let Expression::NumericLiteral(n) = &p.value {
                                return (n.value as usize) < min_nodes;
                            }
            }
            false
        }
        _ => false,
    }
}

fn callback_body_has_content_visibility(source: &str, span: oxc_span::Span) -> bool {
    let text = &source[span.start as usize..span.end as usize];
    text.contains("contentVisibility") || text.contains("content-visibility")
}

fn walk_expr_for_jsx(e: &Expression) -> bool {
    match e {
        Expression::JSXElement(_) => true,
        Expression::ParenthesizedExpression(p) => walk_expr_for_jsx(&p.expression),
        Expression::ConditionalExpression(c) => {
            walk_expr_for_jsx(&c.consequent) || walk_expr_for_jsx(&c.alternate)
        }
        _ => false,
    }
}

fn callback_returns_jsx_in_body(body: &oxc_ast::ast::FunctionBody) -> bool {
    for stmt in &body.statements {
        match stmt {
            oxc_ast::ast::Statement::ExpressionStatement(es) => {
                if walk_expr_for_jsx(&es.expression) {
                    return true;
                }
            }
            oxc_ast::ast::Statement::ReturnStatement(ret) => {
                if let Some(arg) = &ret.argument
                    && walk_expr_for_jsx(arg) {
                        return true;
                    }
            }
            _ => {}
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let min_nodes =
            ctx.config
                .threshold("react-require-content-visibility", "min_nodes", ctx.lang);

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Must be `.map(...)` call
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "map" {
            return;
        }
        if !in_jsx_expression(node, semantic) {
            return;
        }

        let recv = &member.object;
        let known_large = large_array_source(recv, min_nodes);

        if !known_large && is_known_small_array_source(recv, min_nodes) {
            return;
        }
        if enclosing_virtualizer_tag(node, semantic) {
            return;
        }

        // Find the callback argument
        let Some(first_arg) = call.arguments.first() else {
            return;
        };

        let (returns_jsx, has_cv, cb_span) = match first_arg {
            Argument::ArrowFunctionExpression(arrow) => (
                callback_returns_jsx_in_body(&arrow.body),
                callback_body_has_content_visibility(ctx.source, arrow.span),
                arrow.span,
            ),
            Argument::FunctionExpression(func) => {
                let body = match &func.body {
                    Some(b) => b,
                    None => return,
                };
                (
                    callback_returns_jsx_in_body(body),
                    callback_body_has_content_visibility(ctx.source, func.span()),
                    func.span(),
                )
            }
            _ => return,
        };

        if !known_large && !returns_jsx {
            return;
        }

        if has_cv {
            return;
        }
        let _ = cb_span;

        let msg = if known_large {
            format!(
                "Large list rendered with `.map()` (>= {min_nodes} items) in JSX without \
                 virtualization or `contentVisibility: 'auto'` — paints every off-screen row."
            )
        } else {
            "`.map()` rendering JSX in a JSX expression — wrap with a virtualizer or set \
             `contentVisibility: 'auto'` if the array can be long."
                .to_string()
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: msg,
            severity: Severity::Warning,
            span: None,
        });
    }
}
