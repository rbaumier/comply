//! elysia-objectstring-for-query oxc backend — query string fields cannot carry
//! nested `t.Object(...)`; use `t.ObjectString({...})` for JSON-encoded objects.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const STOP_KEYS: &[&str] = &[
    "body:",
    "params:",
    "headers:",
    "response:",
    "cookie:",
    "detail:",
    "tags:",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Get the full text of the arguments
        let args_start = call.span.start as usize;
        let args_end = call.span.end as usize;
        if args_end > ctx.source.len() {
            return;
        }
        let args_text = &ctx.source[args_start..args_end];
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();

        let Some(idx) = norm.find("query:t.Object({") else {
            return;
        };
        let after_outer = &norm[idx + "query:t.Object({".len()..];

        let cut = STOP_KEYS
            .iter()
            .filter_map(|k| after_outer.find(k))
            .min()
            .unwrap_or(after_outer.len());
        let section = &after_outer[..cut];

        if !section.contains("t.Object(") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Nested `t.Object(...)` in a `query:` schema cannot validate query strings — use `t.ObjectString({...})`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
