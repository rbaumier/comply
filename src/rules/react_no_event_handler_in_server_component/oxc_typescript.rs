//! react-no-event-handler-in-server-component OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::file_ctx::RscContext;
use oxc_ast::ast::JSXAttributeName;
use std::sync::Arc;

fn is_event_handler(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next() == Some('o')
        && chars.next() == Some('n')
        && chars.next().is_some_and(|c| c.is_ascii_uppercase())
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
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

        let AstKind::JSXAttribute(attr) = node.kind() else {
            return;
        };
        let JSXAttributeName::Identifier(name_ident) = &attr.name else {
            return;
        };
        let attr_name = name_ident.name.as_str();
        if !is_event_handler(attr_name) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{attr_name}` is a client-side event handler. Server components \
                 can't attach them — move this JSX into a `\"use client\"` \
                 component or use `<form action={{...}}>` for submits."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
