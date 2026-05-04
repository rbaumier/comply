//! timeout-on-io OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const IO_CALLEE_BASES: &[&str] = &["fetch", "axios", "http", "https", "db"];
const IO_METHOD_SUFFIXES: &[&str] = &[
    "query", "execute", "get", "post", "put", "delete", "patch", "request", "send",
];

fn is_test_path(path: &std::path::Path) -> bool {
    let lower = path.to_string_lossy().replace('\\', "/");
    lower.starts_with("tests/")
        || lower.starts_with("test/")
        || lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.contains("/__tests__/")
        || lower.contains(".test.")
        || lower.contains(".spec.")
}

/// Check if a call expression's callee is a known I/O pattern.
fn is_io_callee(callee: &Expression, source: &str) -> bool {
    let text = &source[callee.span().start as usize..callee.span().end as usize];

    // Bare identifier: `fetch(...)`
    if IO_CALLEE_BASES.contains(&text) {
        return true;
    }

    // Dotted member: `foo.query`, `db.get`, `axios.post`
    if let Some((base, method)) = text.rsplit_once('.') {
        if IO_CALLEE_BASES
            .iter()
            .any(|b| base == *b || base.ends_with(&format!(".{b}")))
            && IO_METHOD_SUFFIXES.contains(&method)
        {
            return true;
        }
        if IO_METHOD_SUFFIXES.contains(&method) && base.to_ascii_lowercase().contains("db") {
            return true;
        }
    }
    false
}

/// Check if call arguments contain AbortSignal or timeout option.
fn has_abort_signal_or_timeout(call: &oxc_ast::ast::CallExpression, source: &str) -> bool {
    let args_text = &source[call.span.start as usize..call.span.end as usize];
    args_text.contains("AbortSignal") || args_text.contains("signal:") || args_text.contains("timeout:")
}

/// Check if the await expression is wrapped in a withTimeout/raceTimeout call.
fn is_wrapped_in_timeout<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let mut cur_id = node.id();
    loop {
        let parent_id = nodes.parent_id(cur_id);
        if parent_id == cur_id {
            break;
        }
        let parent_kind = nodes.kind(parent_id);
        if let AstKind::CallExpression(call) = parent_kind {
            let callee_text =
                &source[call.callee.span().start as usize..call.callee.span().end as usize];
            if callee_text.contains("withTimeout") || callee_text.contains("raceTimeout") {
                return true;
            }
        }
        cur_id = parent_id;
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AwaitExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["await"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.path_segments.in_test_dir || is_test_path(ctx.path) {
            return;
        }

        let AstKind::AwaitExpression(await_expr) = node.kind() else {
            return;
        };

        // The awaited expression must be a call expression.
        let Expression::CallExpression(call) = &await_expr.argument else {
            return;
        };

        if !is_io_callee(&call.callee, ctx.source) {
            return;
        }

        if has_abort_signal_or_timeout(call, ctx.source) {
            return;
        }

        if is_wrapped_in_timeout(node, semantic, ctx.source) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "I/O call without a timeout — network calls can \
                      hang forever. Wrap with `withTimeout(..., 5_000)` \
                      or pass `{ signal: AbortSignal.timeout(5_000) }`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
