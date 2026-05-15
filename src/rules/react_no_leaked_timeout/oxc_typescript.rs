//! react-no-leaked-timeout oxc backend.

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
        Some(&["setTimeout"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (body, body_text) in use_effect_bodies(semantic, ctx.source) {
            if !body_text.contains("setTimeout(") {
                continue;
            }
            if body_returns_cleanup(body, ctx.source, &["clearTimeout"]) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, body.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`setTimeout` in `useEffect` without `clearTimeout` cleanup — \
                          the timeout may fire after the component unmounts."
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
    fn flags_set_timeout_without_cleanup() {
        let src = r#"
            function C() {
                useEffect(() => {
                    setTimeout(() => doStuff(), 1000);
                }, []);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_set_timeout_with_cleanup() {
        let src = r#"
            function C() {
                useEffect(() => {
                    const id = setTimeout(() => doStuff(), 1000);
                    return () => clearTimeout(id);
                }, []);
            }
        "#;
        assert!(run(src).is_empty());
    }
}
