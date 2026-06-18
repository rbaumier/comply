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
        semantic: &'a oxc_semantic::Semantic<'a>,
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

        // Only the native HTML `<form>` element fires a real DOM submit event
        // that needs `preventDefault()`. On a PascalCase React component
        // (e.g. `<Form onSubmit={...}>`) the `onSubmit` prop is a
        // library-defined data callback whose submission handling — including
        // `preventDefault` — lives inside the component, so it must not be
        // flagged.
        if !parent_is_native_form(node, semantic) {
            return;
        }

        let Some(ref value) = attr.value else {
            return;
        };
        let JSXAttributeValue::ExpressionContainer(container) = value else {
            return;
        };
        // Only inspect inline handlers (arrow / function expression).
        let (param_name, body_source, expr_start, body_delegates) =
            match &container.expression {
                JSXExpression::ArrowFunctionExpression(arrow) => {
                    let pname = first_param_name(&arrow.params);
                    let pname = match pname {
                        Some(n) => n,
                        None => return,
                    };
                    let body_src = &ctx.source[arrow.body.span.start as usize..arrow.body.span.end as usize];
                    let delegates = arrow_body_delegates(arrow, &pname);
                    (pname, body_src, arrow.span.start, delegates)
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
                    let delegates = function_body_delegates(body, &pname);
                    (pname, body_src, func.span.start, delegates)
                }
                _ => return,
            };

        // Check if body contains `<param>.preventDefault()`.
        let needle = format!("{param_name}.preventDefault(");
        if body_source.contains(&needle) {
            return;
        }

        // Delegation: body forwards the event to another handler (e.g.
        // `(e) => form.handleSubmit(onSubmit)(e)` /
        // `(e) => void flow.onSubmit(e)`). RHF's `form.handleSubmit` and
        // similar wrappers call preventDefault internally, and a
        // forwarding handler is the documented shape. Leading event-identity
        // bubble guards (`if (e.target !== e.currentTarget) return;`) are
        // allowed before the delegation — they only bail on submit events
        // that bubbled up from a nested, portaled child form.
        if body_delegates {
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

/// True when the attribute's enclosing JSX opening element is the native
/// lowercase HTML `<form>` element. PascalCase components and any other tag
/// resolve to `false`.
fn parent_is_native_form<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let AstKind::JSXOpeningElement(opening) = semantic.nodes().parent_node(node.id()).kind() else {
        return false;
    };
    let JSXElementName::Identifier(tag_ident) = &opening.name else {
        return false;
    };
    tag_ident.name.as_str() == "form"
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

fn arrow_body_delegates(arrow: &ArrowFunctionExpression, param: &str) -> bool {
    if arrow.expression {
        // A concise-body arrow is a single expression — no room for a
        // leading guard, so just check the delegation shape directly.
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
    function_body_delegates(&arrow.body, param)
}

/// True when the body is zero or more leading event-identity bubble guards
/// followed by a final delegation statement. With no leading statements this
/// reduces to "the single statement is a delegation".
fn function_body_delegates(body: &FunctionBody, param: &str) -> bool {
    let Some((last, leading)) = body.statements.split_last() else {
        return false;
    };
    if !leading.iter().all(|stmt| is_event_bubble_guard(stmt, param)) {
        return false;
    }
    match last {
        Statement::ExpressionStatement(es) => is_delegation_shape(&es.expression),
        Statement::ReturnStatement(ret) => ret
            .argument
            .as_ref()
            .is_some_and(is_delegation_shape),
        _ => false,
    }
}

/// True when `stmt` is `if (<param>.target !== <param>.currentTarget) return;`
/// (operands in either order, with or without a block around the `return`).
/// Such a guard only bails on submit events that bubbled up from a nested,
/// portaled child form — it never leaves this form's default unprevented. A
/// generic validation guard (e.g. `if (isInvalid) return;`) is NOT matched.
fn is_event_bubble_guard(stmt: &Statement, param: &str) -> bool {
    let Statement::IfStatement(if_stmt) = stmt else {
        return false;
    };
    if if_stmt.alternate.is_some() {
        return false;
    }
    if !consequent_is_bare_return(&if_stmt.consequent) {
        return false;
    }
    let Expression::BinaryExpression(bin) = &if_stmt.test else {
        return false;
    };
    if bin.operator != BinaryOperator::StrictInequality {
        return false;
    }
    let (Some(left), Some(right)) = (
        event_member_property(&bin.left, param),
        event_member_property(&bin.right, param),
    ) else {
        return false;
    };
    (left == "target" && right == "currentTarget")
        || (left == "currentTarget" && right == "target")
}

/// True when `stmt` is a bare `return;` — directly, or as the sole statement
/// of a block.
fn consequent_is_bare_return(stmt: &Statement) -> bool {
    match stmt {
        Statement::ReturnStatement(_) => true,
        Statement::BlockStatement(block) => {
            matches!(block.body.as_slice(), [Statement::ReturnStatement(_)])
        }
        _ => false,
    }
}

/// For `<param>.target` / `<param>.currentTarget`, returns the property name.
/// Returns `None` for any other expression.
fn event_member_property<'a>(expr: &'a Expression, param: &str) -> Option<&'a str> {
    let Expression::StaticMemberExpression(member) = expr else {
        return None;
    };
    let Expression::Identifier(obj) = &member.object else {
        return None;
    };
    if obj.name.as_str() != param {
        return None;
    }
    Some(member.property.name.as_str())
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
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

    #[test]
    fn allows_bubble_guard_then_delegation() {
        // Regression for rbaumier/comply#4200 — a leading event-identity
        // bubble guard before delegating to RHF's wrapper.
        let src = r#"const f = (
          <form
            onSubmit={(event) => {
              if (event.target !== event.currentTarget) {
                return;
              }
              void onSubmit(event);
            }}
          />
        );"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bubble_guard_symmetric_operands() {
        let src = r#"const f = (
          <form
            onSubmit={(event) => {
              if (event.currentTarget !== event.target) {
                return;
              }
              void onSubmit(event);
            }}
          />
        );"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bubble_guard_bare_return() {
        let src = r#"const f = (
          <form
            onSubmit={(event) => {
              if (event.target !== event.currentTarget) return;
              void onSubmit(event);
            }}
          />
        );"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_two_bubble_guards_then_delegation() {
        // The exemption accepts zero-or-more leading bubble guards.
        let src = r#"const f = (
          <form
            onSubmit={(event) => {
              if (event.target !== event.currentTarget) {
                return;
              }
              if (event.currentTarget !== event.target) return;
              void onSubmit(event);
            }}
          />
        );"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_validation_guard_then_delegation() {
        // A non-identity guard returns early on a real submit, leaving the
        // default unprevented — must still flag.
        let src = r#"const f = (
          <form
            onSubmit={(event) => {
              if (isInvalid) {
                return;
              }
              void onSubmit(event);
            }}
          />
        );"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bubble_guard_then_non_delegation() {
        // The final statement is not a delegation, so nothing calls
        // preventDefault — must still flag.
        let src = r#"const f = (
          <form
            onSubmit={(event) => {
              if (event.target !== event.currentTarget) {
                return;
              }
              doStuff(event);
            }}
          />
        );"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_onsubmit_on_pascal_case_component() {
        // Regression for rbaumier/comply#1754 — a custom `<Form>` component
        // takes a library-defined onSubmit callback; preventDefault is handled
        // inside the component, not by the caller's handler.
        let src = r#"const f = (
          <Form
            id="create-comment"
            onSubmit={(values) => {
              createCommentMutation.mutate({ data: values });
            }}
            schema={createCommentInputSchema}
          >
            {({ register, formState }) => null}
          </Form>
        );"#;
        assert!(run(src).is_empty());
    }
}
