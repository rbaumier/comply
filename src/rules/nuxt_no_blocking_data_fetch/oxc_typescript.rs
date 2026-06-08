//! nuxt-no-blocking-data-fetch OXC backend.
//!
//! Flags `fetch(...)`, `$fetch(...)`, `useFetch(...)`, `useAsyncData(...)`
//! inside the body of a `defineNuxtRouteMiddleware` callback.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;

pub struct Check;

const BLOCKING_CALLS: &[&str] = &["fetch", "$fetch", "useFetch", "useAsyncData"];

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["defineNuxtRouteMiddleware"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Check callee is one of the blocking calls.
        let name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if !BLOCKING_CALLS.contains(&name) {
            return;
        }

        // Walk ancestors to see if inside defineNuxtRouteMiddleware.
        let mut in_middleware = false;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if let AstKind::CallExpression(parent_call) = ancestor.kind() {
                if let Expression::Identifier(id) = &parent_call.callee {
                    if id.name.as_str() == "defineNuxtRouteMiddleware" {
                        in_middleware = true;
                        break;
                    }
                }
            }
        }
        if !in_middleware {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{name}()` inside route middleware blocks navigation — fetch in the page's `setup()` instead."
            ),
            severity: Severity::Warning,
            span: Some((call.span.start as usize, (call.span.end - call.span.start) as usize)),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_use_fetch_in_middleware() {
        let src = "export default defineNuxtRouteMiddleware(async () => { const { data } = await useFetch('/api/me'); });";
        assert!(!run_on(src).is_empty());
    }


    #[test]
    fn flags_raw_fetch_in_middleware() {
        let src = "export default defineNuxtRouteMiddleware(async () => { const r = await fetch('/api/me'); });";
        assert!(!run_on(src).is_empty());
    }


    #[test]
    fn allows_in_setup() {
        let src = "export default defineComponent({ async setup() { const { data } = await useFetch('/api/me'); return { data }; } });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_pure_middleware() {
        let src = "export default defineNuxtRouteMiddleware((to) => { if (!to.params.id) return navigateTo('/'); });";
        assert!(run_on(src).is_empty());
    }
}
