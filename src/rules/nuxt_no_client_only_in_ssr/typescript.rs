//! nuxt-no-client-only-in-ssr backend.
//!
//! Flags top-level reads of `window`, `document`, `localStorage`,
//! `sessionStorage`, or `navigator` in a Nuxt file. These globals only
//! exist in the browser; touching them at module scope crashes the SSR
//! render. Allowed only inside any function (developer is responsible for
//! `onMounted` etc.) or behind a `process.client` / `import.meta.client`
//! branch.

use crate::diagnostic::{Diagnostic, Severity};

const BROWSER_GLOBALS: &[&str] = &[
    "window",
    "document",
    "localStorage",
    "sessionStorage",
    "navigator",
];

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

crate::ast_check! { on ["identifier"] => |node, source, ctx, diagnostics|
    if !is_nuxt_source(ctx.source) {
        return;
    }
    let Ok(name) = node.utf8_text(source) else { return };
    if !BROWSER_GLOBALS.contains(&name) {
        return;
    }
    let Some(parent) = node.parent() else { return };
    if parent.kind() == "member_expression" {
        if let Some(prop) = parent.child_by_field_name("property") {
            if prop.id() == node.id() {
                return;
            }
        }
    }
    if parent.kind() == "variable_declarator" {
        return;
    }
    if parent.kind() == "property_identifier" {
        return;
    }
    if parent.kind() == "shorthand_property_identifier" {
        return;
    }

    let mut p = node.parent();
    let mut depth = 0;
    while let Some(parent) = p {
        let kind = parent.kind();
        if kind == "function_declaration"
            || kind == "method_definition"
            || kind == "arrow_function"
            || kind == "function_expression"
        {
            return;
        }
        if kind == "if_statement" {
            if let Some(cond) = parent.child_by_field_name("condition") {
                if let Ok(cond_text) = cond.utf8_text(source) {
                    if cond_text.contains("process.client")
                        || cond_text.contains("import.meta.client")
                    {
                        return;
                    }
                }
            }
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

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "nuxt-no-client-only-in-ssr".into(),
        message: format!(
            "`{name}` is browser-only — guard with `if (import.meta.client)` or move into `onMounted`."
        ),
        severity: Severity::Error,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_top_level_window_access() {
        let src = "import {} from '#imports';\nconst w = window.innerWidth;";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn flags_top_level_localstorage() {
        let src = "import {} from '#imports';\nconst v = localStorage.getItem('k');";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn allows_inside_function() {
        let src = "import {} from '#imports';\nfunction read() { return window.innerWidth; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_with_process_client_guard() {
        let src = "import {} from '#imports';\nif (process.client) { const w = window.innerWidth; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_nuxt_files() {
        let src = "const w = window.innerWidth;";
        assert!(run_on(src).is_empty());
    }
}
