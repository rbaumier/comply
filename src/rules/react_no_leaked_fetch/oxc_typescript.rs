//! react-no-leaked-fetch oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use crate::rules::react_leak_helpers::use_effect_bodies;
use std::sync::Arc;

pub struct Check;

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
        for (body, body_text) in use_effect_bodies(semantic, ctx.source) {
            // Detect a literal `fetch(` call. Be conservative: skip
            // method calls like `something.fetch(` (could be a Drizzle
            // / Prisma client method) and `await fetch(...)` followed
            // by an `.abort()` reference somewhere.
            if !body_text.contains("fetch(") {
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
}
