//! react-no-leaked-fetch oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use crate::rules::react_leak_helpers::use_effect_bodies;
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Byte span of a call expression, used to test whether a global `fetch(...)`
/// call sits inside a `useEffect` callback body.
type CallSpan = (u32, u32);

/// True when `callee` is the browser global `fetch`: a bare `fetch` identifier,
/// or an explicit global member access `window.fetch` / `globalThis.fetch` /
/// `self.fetch`. A `.fetch(...)` method call on any other object (tRPC
/// `utils.post.all.fetch`, a Drizzle / Prisma client, …) is not the global and
/// returns `false`.
fn callee_is_global_fetch(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => id.name == "fetch",
        Expression::StaticMemberExpression(member) => {
            member.property.name == "fetch"
                && matches!(
                    &member.object,
                    Expression::Identifier(obj)
                        if matches!(obj.name.as_str(), "window" | "globalThis" | "self")
                )
        }
        _ => false,
    }
}

/// Byte spans of every global `fetch(...)` call in the file.
fn global_fetch_call_spans(semantic: &oxc_semantic::Semantic) -> Vec<CallSpan> {
    semantic
        .nodes()
        .iter()
        .filter_map(|node| {
            let oxc_ast::AstKind::CallExpression(call) = node.kind() else {
                return None;
            };
            callee_is_global_fetch(&call.callee).then_some((call.span.start, call.span.end))
        })
        .collect()
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["fetch("])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let fetch_spans = global_fetch_call_spans(semantic);
        for (body, body_text) in use_effect_bodies(semantic, ctx.source) {
            // Only flag the browser global `fetch(...)`. Method calls like
            // `utils.post.all.fetch(...)` (tRPC / Drizzle / Prisma clients) are
            // not the global and are detected by AST callee, not text — so they
            // are skipped here.
            let body_span = (body.span.start, body.span.end);
            let has_global_fetch = fetch_spans
                .iter()
                .any(|&(start, end)| start >= body_span.0 && end <= body_span.1);
            if !has_global_fetch {
                continue;
            }
            // Heuristic for "this fetch is cancellable": the body
            // mentions either an AbortController, a `.signal` property
            // passed to fetch, or a cleanup returning `.abort()`.
            if body_text.contains("AbortController")
                || body_text.contains(".abort(")
                || body_text.contains("signal:")
                || body_text.contains("signal ")
            {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, body.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`fetch(...)` in `useEffect` without an AbortController signal — \
                          requests cannot be cancelled if the component unmounts mid-flight."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_fetch_without_abort_controller() {
        let src = r#"
            function C() {
                useEffect(() => {
                    fetch("/api").then(r => r.json());
                }, []);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_fetch_with_abort_controller() {
        let src = r#"
            function C() {
                useEffect(() => {
                    const c = new AbortController();
                    fetch("/api", { signal: c.signal });
                    return () => c.abort();
                }, []);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_window_fetch_without_abort_controller() {
        let src = r#"
            function C() {
                useEffect(() => {
                    window.fetch("/api").then(r => r.json());
                }, []);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_chained_fetch_method_call() {
        let src = r#"
            function C() {
                useEffect(() => {
                    utils.post.all
                        .fetch(undefined, { trpc: { context } })
                        .then((allPosts) => {
                            setPosts(allPosts);
                        });
                }, []);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_prefetch_method_call() {
        let src = r#"
            function C() {
                useEffect(() => {
                    utils.postById.prefetch(1);
                }, []);
            }
        "#;
        assert!(run(src).is_empty());
    }
}
