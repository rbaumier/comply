//! elysia-no-server-assertion oxc backend — flag `server!` non-null assertions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSNonNullExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["server!"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSNonNullExpression(expr) = node.kind() else {
            return;
        };
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let text = &ctx.source[expr.span.start as usize..expr.span.end as usize];
        // text looks like `something!` — check that it ends with `server!` or `.server!`.
        if !(text.ends_with(".server!") || text == "server!") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`server!` non-null assertion is unsafe — `app.server` is undefined until `.listen()` resolves.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
