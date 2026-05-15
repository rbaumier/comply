use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["onSubmit"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else {
            return;
        };
        let JSXAttributeName::Identifier(name_ident) = &attr.name else {
            return;
        };
        if name_ident.name.as_str() != "onSubmit" {
            return;
        }

        let Some(ref value) = attr.value else {
            return;
        };
        let JSXAttributeValue::ExpressionContainer(container) = value else {
            return;
        };
        // Only inspect inline handlers (arrow / function expression).
        let (param_name, body_source, expr_start, body_is_single_call) =
            match &container.expression {
                JSXExpression::ArrowFunctionExpression(arrow) => {
                    let pname = first_param_name(&arrow.params);
                    let pname = match pname {
                        Some(n) => n,
                        None => return,
                    };
                    let body_src = &ctx.source[arrow.body.span.start as usize..arrow.body.span.end as usize];
                    let single_call = arrow_body_is_single_call(arrow);
                    (pname, body_src, arrow.span.start, single_call)
                }
                JSXExpression::FunctionExpression(func) => {
                    let pname = first_param_name(&func.params);
                    let pname = match pname {
                        Some(n) => n,
                        None => return,
                    };
                    let Some(ref body) = func.body else {
                        return;
                    };
                    let body_src = &ctx.source[body.span.start as usize..body.span.end as usize];
                    let single_call = function_body_is_single_call(body);
                    (pname, body_src, func.span.start, single_call)
                }
                _ => return,
            };

        // Check if body contains `<param>.preventDefault()`.
        let needle = format!("{param_name}.preventDefault(");
        if body_source.contains(&needle) {
            return;
        }

        // Delegation: body is a single call expression (e.g.
        // `(e) => form.handleSubmit(onSubmit)(e)` /
        // `(e) => void flow.onSubmit(e)`). The author has explicitly
        // forwarded the event to another handler — RHF's
        // \`form.handleSubmit\` and similar wrappers call preventDefault
        // internally, and a forwarding handler is the documented shape.
        if body_is_single_call {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, expr_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`onSubmit` handler does not call `preventDefault()` \u{2014} the browser will perform a full-page submit and reset the form.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn first_param_name(params: &FormalParameters) -> Option<String> {
    let first = params.items.first()?;
    match &first.pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
        _ => None,
    }
}

/// True if `expr` is one of the documented "delegation" shapes:
/// - `void <CallExpression>` — explicit cast signalling the author
///   knows the inner call handles the event.
/// - A call whose callee is `*.handleSubmit(...)` (React Hook Form's
///   wrapper) at any depth, with or without a trailing `(event)` call.
fn is_delegation_shape(expr: &Expression) -> bool {
    match expr {
        Expression::UnaryExpression(u)
            if u.operator == oxc_ast::ast::UnaryOperator::Void =>
        {
            // `void <anything that calls a function>` — explicit
            // delegation cast.
            matches!(
                strip_wrappers(&u.argument),
                Expression::CallExpression(_) | Expression::AwaitExpression(_)
            )
        }
        Expression::ParenthesizedExpression(p) => is_delegation_shape(&p.expression),
        Expression::CallExpression(call) => callee_is_handle_submit(&call.callee),
        _ => false,
    }
}

fn strip_wrappers<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    let mut current = expr;
    loop {
        match current {
            Expression::ParenthesizedExpression(p) => current = &p.expression,
            Expression::AwaitExpression(a) => current = &a.argument,
            _ => return current,
        }
    }
}

fn callee_is_handle_submit(expr: &Expression) -> bool {
    let mut current = strip_wrappers(expr);
    // Drill through `<x>.handleSubmit(...)(e)` — the outer call's callee
    // is itself a CallExpression whose callee is `.handleSubmit`.
    loop {
        match current {
            Expression::StaticMemberExpression(member) => {
                return member.property.name.as_str() == "handleSubmit";
            }
            Expression::CallExpression(c) => current = strip_wrappers(&c.callee),
            _ => return false,
        }
    }
}

fn arrow_body_is_single_call(arrow: &ArrowFunctionExpression) -> bool {
    if arrow.expression {
        return arrow
            .body
            .statements
            .first()
            .and_then(|s| match s {
                Statement::ExpressionStatement(es) => Some(&es.expression),
                _ => None,
            })
            .is_some_and(is_delegation_shape);
    }
    function_body_is_single_call(&arrow.body)
}

fn function_body_is_single_call(body: &FunctionBody) -> bool {
    if body.statements.len() != 1 {
        return false;
    }
    match &body.statements[0] {
        Statement::ExpressionStatement(es) => is_delegation_shape(&es.expression),
        Statement::ReturnStatement(ret) => ret
            .argument
            .as_ref()
            .is_some_and(is_delegation_shape),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(src, &Check)
    }

    #[test]
    fn flags_inline_handler_without_prevent_default() {
        let src = r#"const f = <form onSubmit={(e) => { console.log(e); }} />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_explicit_prevent_default() {
        let src = r#"const f = <form onSubmit={(e) => { e.preventDefault(); doStuff(); }} />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_void_forward_to_handler() {
        // Regression for rbaumier/comply#20 — RHF / wrapper delegation.
        let src = r#"const f = <form onSubmit={(e) => void flow.onSubmit(e)} />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_call_expression_pass_through() {
        let src = r#"const f = <form onSubmit={form.handleSubmit(onSubmit)} />;"#;
        // This goes through the non-arrow path which already passes.
        assert!(run(src).is_empty());
    }
}
