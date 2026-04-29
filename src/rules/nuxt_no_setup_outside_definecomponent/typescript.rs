//! nuxt-no-setup-outside-definecomponent backend.
//!
//! Flags top-level calls to setup-only composables (`useState`, `useFetch`,
//! `useAsyncData`, `useRuntimeConfig`, `useNuxtApp`) that appear at module
//! scope in a file that uses options API (`export default { ... }` without
//! `defineComponent`). Outside of a Vue setup context these calls have no
//! current instance and cause SSR cross-request bleed.

use crate::diagnostic::{Diagnostic, Severity};

fn is_nuxt_options_api(src: &str) -> bool {
    let nuxt = src.contains("#imports")
        || src.contains("nuxt/app")
        || src.contains("#app")
        || src.contains("defineNuxtPlugin")
        || src.contains("defineNuxtRouteMiddleware");
    if !nuxt {
        return false;
    }
    src.contains("export default {") && !src.contains("defineComponent(")
}

const SETUP_COMPOSABLES: &[&str] = &[
    "useState",
    "useFetch",
    "useAsyncData",
    "useNuxtApp",
    "useRuntimeConfig",
    "useRoute",
    "useRouter",
];

crate::ast_check! { on ["call_expression"] prefilter = ["setup"] => |node, source, ctx, diagnostics|
    if !is_nuxt_options_api(ctx.source) {
        return;
    }
    let mut p = node.parent();
    let mut depth = 0;
    while let Some(parent) = p {
        if parent.kind() == "function_declaration"
            || parent.kind() == "method_definition"
            || parent.kind() == "arrow_function"
            || parent.kind() == "function_expression"
        {
            return;
        }
        if parent.kind() == "program" {
            break;
        }
        depth += 1;
        if depth > 6 {
            return;
        }
        p = parent.parent();
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let Ok(name) = callee.utf8_text(source) else { return };
    if !SETUP_COMPOSABLES.contains(&name) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "nuxt-no-setup-outside-definecomponent".into(),
        message: format!(
            "`{name}()` called at module scope in an options-API file — wrap in `defineComponent({{ setup() {{ ... }} }})` or use `<script setup>`."
        ),
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
    fn flags_use_state_at_module_scope_in_options_api() {
        let src = "import {} from '#imports';\nconst s = useState('x', () => 0);\nexport default { name: 'X' };";
        assert!(!run_on(src).is_empty());
    }

    #[test]
    fn allows_inside_define_component() {
        let src = "import {} from '#imports';\nexport default defineComponent({ setup() { const s = useState('x', () => 0); return { s }; } });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_nuxt_files() {
        let src = "const s = useState('x');\nexport default { name: 'X' };";
        assert!(run_on(src).is_empty());
    }
}
