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
        let (param_name, body_source, expr_start) = match &container.expression {
            JSXExpression::ArrowFunctionExpression(arrow) => {
                let pname = first_param_name(&arrow.params);
                let pname = match pname {
                    Some(n) => n,
                    None => return,
                };
                let body_src = &ctx.source[arrow.body.span.start as usize..arrow.body.span.end as usize];
                (pname, body_src, arrow.span.start)
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
                (pname, body_src, func.span.start)
            }
            _ => return,
        };

        // Check if body contains `<param>.preventDefault()`.
        let needle = format!("{param_name}.preventDefault(");
        if body_source.contains(&needle) {
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
