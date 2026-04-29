//! nuxt-no-blocking-data-fetch backend.
//!
//! Flags `await fetch(...)`, `await $fetch(...)`, `await useFetch(...)`,
//! `await useAsyncData(...)` inside the body of a
//! `defineNuxtRouteMiddleware` callback. Awaiting data in middleware
//! delays navigation for every route hit.

use crate::diagnostic::{Diagnostic, Severity};

const BLOCKING_CALLS: &[&str] = &["fetch", "$fetch", "useFetch", "useAsyncData"];

crate::ast_check! { on ["call_expression"] prefilter = ["defineNuxtRouteMiddleware"] => |node, source, ctx, diagnostics|
    let mut p = node.parent();
    let mut in_middleware = false;
    let mut depth = 0;
    while let Some(parent) = p {
        if parent.kind() == "call_expression" {
            if let Some(callee) = parent.child_by_field_name("function") {
                if let Ok(name) = callee.utf8_text(source) {
                    if name == "defineNuxtRouteMiddleware" {
                        in_middleware = true;
                        break;
                    }
                }
            }
        }
        depth += 1;
        if depth > 12 {
            return;
        }
        p = parent.parent();
    }
    if !in_middleware {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let Ok(name) = callee.utf8_text(source) else { return };
    if !BLOCKING_CALLS.contains(&name) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "nuxt-no-blocking-data-fetch".into(),
        message: format!(
            "`{name}()` inside route middleware blocks navigation — fetch in the page's `setup()` instead."
        ),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
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
