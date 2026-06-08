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

/// True when the options argument is an opaque reference the caller controls —
/// a forwarded `init`/options identifier, a member access, a spread argument, or
/// an object literal that spreads such a value in (`{ ...base }`). Any of these
/// may already carry a `signal`, and hardcoding a timeout inside a pass-through
/// wrapper would override whatever the caller supplied.
fn forwards_opaque_options(call: &oxc_ast::ast::CallExpression) -> bool {
    use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind};
    // Skip the first positional argument (the URL / endpoint); inspect the rest.
    call.arguments.iter().skip(1).any(|arg| match arg {
        Argument::SpreadElement(_) => true,
        _ => match arg.as_expression() {
            Some(
                Expression::Identifier(_)
                | Expression::StaticMemberExpression(_)
                | Expression::ComputedMemberExpression(_),
            ) => true,
            // `{ ...base }` — the spread source may carry a `signal`.
            Some(Expression::ObjectExpression(obj)) => obj
                .properties
                .iter()
                .any(|p| matches!(p, ObjectPropertyKind::SpreadProperty(_))),
            _ => false,
        },
    })
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

        if forwards_opaque_options(call) {
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_bare_fetch() {
        assert_eq!(run("async function f() { await fetch(url); }").len(), 1);
    }

    #[test]
    fn allows_fetch_with_inline_signal() {
        let src = "async function f() { await fetch(url, { signal: AbortSignal.timeout(5000) }); }";
        assert!(run(src).is_empty());
    }

    // Regression for #545: a wrapper forwarding a caller-supplied `init`
    // identifier may already carry a `signal`; the rule cannot introspect it
    // and must not demand a hardcoded timeout that would override the caller.
    #[test]
    fn allows_fetch_forwarding_init_identifier_issue_545() {
        let src = "async function f(input, init) { await fetch(input, init); }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_fetch_forwarding_options_member_issue_545() {
        let src = "async function f(opts) { await fetch('/api', opts.requestInit); }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_fetch_with_spread_options_issue_545() {
        let src = "async function f(base) { await fetch('/api', { ...base }); }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // An inline options object without a signal is still flagged — nothing
    // opaque is forwarded, so the author could add a timeout here.
    #[test]
    fn still_flags_inline_options_without_signal() {
        let src = "async function f() { await fetch('/api', { method: 'POST' }); }";
        assert_eq!(run(src).len(), 1);
    }
}
