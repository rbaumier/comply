//! react-no-leaked-resize-observer oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use crate::rules::react_leak_helpers::{body_returns_cleanup, use_effect_bodies};
use std::sync::Arc;

pub struct Check;

const OBSERVER_TYPES: &[&str] = &[
    "ResizeObserver",
    "IntersectionObserver",
    "MutationObserver",
    "PerformanceObserver",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Observer"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (body, body_text) in use_effect_bodies(semantic, ctx.source) {
            let observer = OBSERVER_TYPES
                .iter()
                .find(|name| body_text.contains(&format!("new {name}(")));
            let Some(observer) = observer else {
                continue;
            };
            if body_returns_cleanup(body, ctx.source, &[".disconnect("]) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, body.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`new {observer}(...)` in `useEffect` without `.disconnect()` cleanup — \
                     the observer outlives the component and prevents GC of its target."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(src, &Check)
    }

    #[test]
    fn flags_resize_observer_without_cleanup() {
        let src = r#"
            function C() {
                useEffect(() => {
                    const ro = new ResizeObserver(handler);
                    ro.observe(el);
                }, []);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_resize_observer_with_disconnect() {
        let src = r#"
            function C() {
                useEffect(() => {
                    const ro = new ResizeObserver(handler);
                    ro.observe(el);
                    return () => ro.disconnect();
                }, []);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_intersection_observer() {
        let src = r#"
            function C() {
                useEffect(() => {
                    const io = new IntersectionObserver(handler);
                }, []);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
