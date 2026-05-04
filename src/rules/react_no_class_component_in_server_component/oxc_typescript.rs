//! react-no-class-component-in-server-component OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::file_ctx::RscContext;
use oxc_span::GetSpan;
use std::sync::Arc;

const REACT_BASES: &[&str] = &["Component", "PureComponent"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
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

        let AstKind::Class(class) = node.kind() else {
            return;
        };

        let Some(super_class) = &class.super_class else {
            return;
        };

        // Get the source text of the super class to extract the base name
        let start = super_class.span().start as usize;
        let end = super_class.span().end as usize;
        if end > ctx.source.len() {
            return;
        }
        let super_text = &ctx.source[start..end];
        // Get the last segment after `.` (e.g. "React.Component" -> "Component")
        let base = super_text.rsplit('.').next().unwrap_or(super_text);

        if !REACT_BASES.contains(&base) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, class.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Class components don't render on the server. Rewrite this as \
                      a function component or move it to a `\"use client\"` module."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
