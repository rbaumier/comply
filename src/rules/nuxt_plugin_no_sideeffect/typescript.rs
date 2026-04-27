//! nuxt-plugin-no-sideeffect backend.
//!
//! Triggers on Nuxt plugin files (path contains `/plugins/`) that have
//! top-level executable statements (call expressions, assignments) outside
//! of a `defineNuxtPlugin(...)` invocation. Such statements run before
//! Nuxt's plugin lifecycle and lose access to the app instance.

use crate::diagnostic::{Diagnostic, Severity};

fn is_plugin_file(ctx_path: &std::path::Path) -> bool {
    ctx_path
        .to_str()
        .map(|p| {
            p.contains("/plugins/")
                || p.contains("\\plugins\\")
                || p.starts_with("plugins/")
                || p.starts_with("plugins\\")
        })
        .unwrap_or(false)
}

crate::ast_check! { on ["expression_statement"] => |node, source, ctx, diagnostics|
    if !is_plugin_file(ctx.path) {
        return;
    }
    let Some(parent) = node.parent() else { return };
    if parent.kind() != "program" {
        return;
    }
    let Ok(text) = node.utf8_text(source) else { return };
    let trimmed = text.trim();
    if trimmed.starts_with("defineNuxtPlugin(")
        || trimmed.starts_with("export default defineNuxtPlugin(")
    {
        return;
    }
    if trimmed.starts_with("import ") || trimmed.starts_with("export ") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "nuxt-plugin-no-sideeffect".into(),
        message: "Top-level side effect in a Nuxt plugin — move it inside `defineNuxtPlugin((nuxtApp) => { ... })`.".into(),
        severity: Severity::Error,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_top_level_call_in_plugin() {
        let src = "console.log('init');\nexport default defineNuxtPlugin(() => {});";
        assert!(!run_on_path(src, "plugins/auth.ts").is_empty());
    }

    #[test]
    fn allows_only_define_nuxt_plugin() {
        let src = "export default defineNuxtPlugin((nuxtApp) => { nuxtApp.provide('x', 1); });";
        assert!(run_on_path(src, "plugins/auth.ts").is_empty());
    }

    #[test]
    fn ignores_non_plugin_files() {
        let src = "console.log('init');";
        assert!(run_on_path(src, "src/utils/log.ts").is_empty());
    }
}
