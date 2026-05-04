use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// Patterns that typically return a promise.
const PROMISE_PATTERNS: &[&str] = &[
    ".then(",
    "fetch(",
    "axios(",
    "axios.get(",
    "axios.post(",
    "axios.put(",
    "axios.delete(",
    "axios.patch(",
];

/// Returns true if the text contains a promise-returning call without `await`.
fn has_unawaited_promise(text: &str) -> bool {
    if text.contains("await ") || text.contains("await(") {
        return false;
    }
    PROMISE_PATTERNS.iter().any(|p| text.contains(p))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["try"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else {
            return;
        };

        let body_start = try_stmt.block.span.start as usize;
        let body_end = try_stmt.block.span.end as usize;
        let body_text = ctx.source.get(body_start..body_end).unwrap_or("");

        // Check each line-ish statement in the try body for unawaited promises.
        // We check the entire body text for promise patterns without await.
        if !has_unawaited_promise(body_text) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, try_stmt.block.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Promise inside try/catch without `await` \u{2014} rejection won't be caught."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
