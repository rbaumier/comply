//! react-no-client-hook-in-server-component OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::file_ctx::RscContext;
use oxc_ast::ast::Expression;
use std::sync::Arc;

fn is_hook_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next() == Some('u')
        && chars.next() == Some('s')
        && chars.next() == Some('e')
        && chars.next().is_some_and(|c| c.is_ascii_uppercase())
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["use"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.rsc_context != RscContext::ServerComponent {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        let name = callee.name.as_str();

        if !is_hook_name(name) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, callee.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{name}()` is a React hook and can't run in a server component. \
                 Mark the file with `\"use client\"` or extract this into a \
                 client component."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
