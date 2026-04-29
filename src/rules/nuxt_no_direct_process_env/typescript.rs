//! nuxt-no-direct-process-env backend.

use crate::diagnostic::{Diagnostic, Severity};

fn is_nuxt_source(src: &str) -> bool {
    src.contains("#imports")
        || src.contains("nuxt/app")
        || src.contains("#app")
        || src.contains("defineNuxtConfig")
        || src.contains("defineNuxtPlugin")
        || src.contains("defineNuxtRouteMiddleware")
        || src.contains("useRuntimeConfig")
        || src.contains("useNuxtApp")
}

crate::ast_check! { on ["member_expression"] prefilter = ["process"] => |node, source, ctx, diagnostics|
    if !is_nuxt_source(ctx.source) {
        return;
    }
    let Ok(text) = node.utf8_text(source) else { return };
    if !(text == "process.env" || text.starts_with("process.env.")) {
        return;
    }
    let Some(object) = node.child_by_field_name("object") else { return };
    let Ok(object_text) = object.utf8_text(source) else { return };
    if object_text != "process" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "nuxt-no-direct-process-env".into(),
        message: "`process.env` is unavailable on the client; use `useRuntimeConfig()` instead.".into(),
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
    fn flags_process_env_access() {
        let src = "import {} from '#imports';\nconst k = process.env.API_KEY;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_use_runtime_config() {
        let src = "import {} from '#imports';\nconst cfg = useRuntimeConfig();\nconst k = cfg.public.apiBase;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_nuxt_files() {
        let src = "const k = process.env.API_KEY;";
        assert!(run_on(src).is_empty());
    }
}
