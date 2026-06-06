//! no-open-redirect oxc backend — flag `res.redirect(userInput)` calls whose
//! argument references request-scoped data (query/params/body).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const REDIRECT_METHODS: &[&str] = &["redirect"];

const USER_DATA_NEEDLES: &[&str] = &[
    "req.query",
    "req.params",
    "req.body",
    "request.query",
    "request.params",
    "request.body",
    "searchParams.get",
];

fn is_redirect_call(name: &str) -> bool {
    let tail = name.rsplit('.').next().unwrap_or(name);
    REDIRECT_METHODS.contains(&tail)
}

fn argument_references_user_data(text: &str) -> bool {
    USER_DATA_NEEDLES.iter().any(|n| text.contains(n))
}

fn callee_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::StaticMemberExpression(m) => {
            let obj = callee_name(&m.object)?;
            Some(format!("{}.{}", obj, m.property.name))
        }
        Expression::Identifier(id) => Some(id.name.to_string()),
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["redirect"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let Some(name) = callee_name(&call.callee) else { return };
        if !is_redirect_call(&name) {
            return;
        }
        for arg in &call.arguments {
            let arg_span = arg.span();
            let text = &ctx.source[arg_span.start as usize..arg_span.end as usize];
            if argument_references_user_data(text) {
                let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Redirect target from user input — validate against an allowlist before redirecting.".into(),
                    severity: Severity::Error,
                    span: None,
                });
                return;
            }
        }
    }
}
