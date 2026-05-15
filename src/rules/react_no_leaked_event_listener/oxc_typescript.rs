//! react-no-leaked-event-listener oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use crate::rules::react_leak_helpers::{body_returns_cleanup, use_effect_bodies};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["addEventListener"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (body, body_text) in use_effect_bodies(semantic, ctx.source) {
            // Find `addEventListener(` in the body — the call may be on
            // window / document / any ref. We don't care about the
            // receiver, only that an addEventListener was created.
            if !body_text.contains(".addEventListener(") {
                continue;
            }
            // If the body returns a function mentioning
            // `removeEventListener`, we trust it cleans up.
            if body_returns_cleanup(body, ctx.source, &["removeEventListener"]) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, body.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`addEventListener` registered in `useEffect` without a \
                          matching `removeEventListener` cleanup — listeners leak on \
                          unmount and re-mount."
                    .into(),
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
    fn flags_add_event_listener_without_cleanup() {
        let src = r#"
            function C() {
                useEffect(() => {
                    window.addEventListener("resize", handler);
                }, []);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_add_event_listener_with_cleanup() {
        let src = r#"
            function C() {
                useEffect(() => {
                    window.addEventListener("resize", handler);
                    return () => window.removeEventListener("resize", handler);
                }, []);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_add_event_listener_outside_use_effect() {
        let src = r#"window.addEventListener("resize", handler);"#;
        assert!(run(src).is_empty());
    }
}
