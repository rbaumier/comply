//! nuxt-no-manual-fetch-in-setup backend.
//!
//! Flags a top-level `fetch(...)` call in a Nuxt file. In `<script setup>`
//! or a setup function, `fetch` runs both on the server (SSR) and again on
//! the client (hydration), producing duplicate requests and inconsistent
//! state. `useFetch` / `useAsyncData` deduplicate via the payload.

use crate::diagnostic::{Diagnostic, Severity};

fn is_nuxt_source(src: &str) -> bool {
    src.contains("#imports")
        || src.contains("nuxt/app")
        || src.contains("#app")
        || src.contains("defineNuxtConfig")
        || src.contains("defineNuxtPlugin")
        || src.contains("defineNuxtRouteMiddleware")
        || src.contains("useNuxtApp")
        || src.contains("useRuntimeConfig")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_nuxt_source(ctx.source) {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    let Ok(name) = callee.utf8_text(source) else { return };
    if name != "fetch" {
        return;
    }
    let mut p = node.parent();
    let mut in_setup = false;
    let mut at_module_scope = true;
    let mut depth = 0;
    while let Some(parent) = p {
        let kind = parent.kind();
        if kind == "method_definition" {
            if let Some(method_name) = parent.child_by_field_name("name") {
                if let Ok(n) = method_name.utf8_text(source) {
                    if n == "setup" {
                        in_setup = true;
                    }
                }
            }
            at_module_scope = false;
        } else if kind == "function_declaration"
            || kind == "arrow_function"
            || kind == "function_expression"
        {
            at_module_scope = false;
        }
        if kind == "program" {
            break;
        }
        depth += 1;
        if depth > 10 {
            return;
        }
        p = parent.parent();
    }

    if !(in_setup || at_module_scope) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "nuxt-no-manual-fetch-in-setup".into(),
        message: "Use `useFetch()` or `useAsyncData()` instead of raw `fetch()` in setup — avoids duplicate SSR + hydration requests.".into(),
        severity: Severity::Error,
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
    fn flags_fetch_at_module_scope() {
        let src = "import {} from '#imports';\nconst data = await fetch('/api/x').then(r => r.json());";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn flags_fetch_inside_setup_method() {
        let src = "import {} from '#imports';\nexport default defineComponent({ async setup() { const r = await fetch('/api/x'); return {}; } });";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn allows_use_fetch() {
        let src = "import {} from '#imports';\nconst { data } = await useFetch('/api/x');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_nuxt_files() {
        let src = "const data = await fetch('/api/x').then(r => r.json());";
        assert!(run_on(src).is_empty());
    }
}
