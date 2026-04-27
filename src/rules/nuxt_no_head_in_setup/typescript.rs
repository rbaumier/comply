//! nuxt-no-head-in-setup backend.
//!
//! Flags a `head:` (or `head() {}`) property on a `defineComponent({ ... })`
//! options object inside a Nuxt file. Nuxt 3 prefers the composable
//! `useHead()` over options-API `head` declarations.

use crate::diagnostic::{Diagnostic, Severity};

fn is_nuxt_source(src: &str) -> bool {
    src.contains("#imports")
        || src.contains("nuxt/app")
        || src.contains("#app")
        || src.contains("defineNuxtConfig")
        || src.contains("defineNuxtPlugin")
        || src.contains("defineNuxtRouteMiddleware")
        || src.contains("useNuxtApp")
        || src.contains("defineComponent")
}

crate::ast_check! { on ["pair", "method_definition"] => |node, source, ctx, diagnostics|
    if !is_nuxt_source(ctx.source) {
        return;
    }
    let mut p = node.parent();
    let mut in_define_component = false;
    let mut depth = 0;
    while let Some(parent) = p {
        if parent.kind() == "call_expression" {
            if let Some(callee) = parent.child_by_field_name("function") {
                if let Ok(name) = callee.utf8_text(source) {
                    if name == "defineComponent" {
                        in_define_component = true;
                        break;
                    }
                }
            }
        }
        depth += 1;
        if depth > 8 {
            return;
        }
        p = parent.parent();
    }
    if !in_define_component {
        return;
    }

    let key = node
        .child_by_field_name("key")
        .or_else(|| node.child_by_field_name("name"));
    let Some(key_node) = key else { return };
    let Ok(key_text) = key_node.utf8_text(source) else { return };
    if key_text != "head" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "nuxt-no-head-in-setup".into(),
        message: "Use `useHead({ ... })` instead of declaring `head` on component options.".into(),
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
    fn flags_head_property_in_define_component() {
        let src = "import {} from '#imports';\nexport default defineComponent({ head: { title: 'X' }, setup() { return {}; } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_head_method_in_define_component() {
        let src = "import {} from '#imports';\nexport default defineComponent({ head() { return { title: 'X' }; }, setup() { return {}; } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_use_head_call() {
        let src = "import {} from '#imports';\nexport default defineComponent({ setup() { useHead({ title: 'X' }); return {}; } });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_nuxt_files() {
        let src = "export default { head: { title: 'X' } };";
        assert!(run_on(src).is_empty());
    }
}
